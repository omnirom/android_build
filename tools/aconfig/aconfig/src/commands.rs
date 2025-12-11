/*
 * Copyright (C) 2023 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use anyhow::{anyhow, bail, ensure, Context, Result};
use convert_finalized_flags::FinalizedFlagMap;
use itertools::Itertools;
use protobuf::Message;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt;
use std::hash::Hasher;
use std::io::Read;
use std::path::PathBuf;

use crate::codegen::cpp::generate_cpp_code;
use crate::codegen::java::{generate_java_code, JavaCodegenConfig};
use crate::codegen::rust::generate_rust_code;
use crate::codegen::CodegenMode;
use crate::dump::{DumpFormat, DumpPredicate};
use crate::storage::generate_storage_file;
use aconfig_protos::{
    ParsedFlagExt, ProtoFlagMetadata, ProtoFlagPermission, ProtoFlagState, ProtoFlagStorageBackend,
    ProtoParsedFlag, ProtoParsedFlags, ProtoTracepoint,
};
use aconfig_storage_file::sip_hasher13::SipHasher13;
use aconfig_storage_file::StorageFileType;

pub struct Input {
    pub source: String,
    pub reader: Box<dyn Read>,
}

impl Input {
    fn try_parse_flags(&mut self) -> Result<ProtoParsedFlags> {
        let mut buffer = Vec::new();
        self.reader
            .read_to_end(&mut buffer)
            .with_context(|| format!("failed to read {}", self.source))?;
        aconfig_protos::parsed_flags::try_from_binary_proto(&buffer)
            .with_context(|| self.error_context())
    }

    fn error_context(&self) -> String {
        format!("failed to parse {}", self.source)
    }
}

impl fmt::Debug for Input {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{}", self.source)
    }
}

#[derive(Debug)]
pub struct OutputFile {
    pub path: PathBuf, // relative to some root directory only main knows about
    pub contents: Vec<u8>,
}

pub const DEFAULT_FLAG_STATE: ProtoFlagState = ProtoFlagState::DISABLED;
pub const DEFAULT_FLAG_PERMISSION: ProtoFlagPermission = ProtoFlagPermission::READ_WRITE;

pub const PLATFORM_CONTAINERS: [&str; 4] = ["system", "system_ext", "product", "vendor"];

#[derive(Serialize, Deserialize, Debug)]
pub struct NamespaceSetting {
    pub container: String,
    pub allow_exported: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MainlineBetaNamespaces {
    pub namespaces: HashMap<String, NamespaceSetting>,
}

#[allow(dead_code)]
impl MainlineBetaNamespaces {
    fn has_flag(&self, pf: &ProtoParsedFlag) -> bool {
        self.namespaces.contains_key(pf.namespace())
    }

    fn is_mainline_beta_flag(&self, pf: &ProtoParsedFlag) -> bool {
        match self.namespaces.get(pf.namespace()) {
            Some(setting) => setting.container == pf.container(),
            None => false,
        }
    }

    // for each mainline beta namespace, only platform and the corresponding
    // module containers are allowed
    fn supports_container(&self, pf: &ProtoParsedFlag) -> bool {
        match self.namespaces.get(pf.namespace()) {
            Some(setting) => {
                setting.container == pf.container()
                    || PLATFORM_CONTAINERS.iter().any(|&c| c == pf.container())
            }
            None => panic!(
                "Should not check container support for flags in non mainline beta namespaces"
            ),
        }
    }

    fn supports_exported_mode(&self, pf: &ProtoParsedFlag) -> bool {
        match self.namespaces.get(pf.namespace()) {
            Some(setting) => {
                if setting.container == pf.container() {
                    setting.allow_exported
                } else {
                    panic!("Should not check exported mode support on none mainline beta flag")
                }
            }
            None => panic!("Should not check exported mode support on none mainline beta flag"),
        }
    }
}

fn assign_storage_backend(
    pf: &mut ProtoParsedFlag,
    beta_namespaces: &Option<MainlineBetaNamespaces>,
) -> Result<()> {
    let is_mainline_beta = match beta_namespaces {
        Some(namespaces) => namespaces.is_mainline_beta_flag(pf),
        None => false,
    };
    let is_read_only = pf.permission() == ProtoFlagPermission::READ_ONLY;
    let storage = if is_read_only {
        ProtoFlagStorageBackend::NONE
    } else if is_mainline_beta {
        ProtoFlagStorageBackend::DEVICE_CONFIG
    } else {
        ProtoFlagStorageBackend::ACONFIGD
    };
    let m = pf.metadata.as_mut().ok_or(anyhow!("missing metadata"))?;
    m.set_storage(storage);
    Ok(())
}

fn verify_mainline_beta_namespace_flag(
    pf: &mut ProtoParsedFlag,
    beta_namespaces: &Option<MainlineBetaNamespaces>,
) -> Result<()> {
    if let Some(namespaces) = beta_namespaces {
        if !namespaces.has_flag(pf) {
            return Ok(());
        }
        ensure!(
            namespaces.supports_container(pf),
            "Creating {} container flag in namespace {} is not allowed",
            pf.container(),
            pf.namespace()
        );
        if pf.is_exported() {
            ensure!(
                namespaces.supports_exported_mode(pf),
                "Creating exported flag {} in namespace {} is not allowed",
                pf.fully_qualified_name(),
                pf.namespace()
            );
        }
    }
    Ok(())
}

pub struct ExtendedPermissionsOptions {
    pub default_permission: ProtoFlagPermission,
    pub allow_read_write: bool,
    pub force_read_only: bool,
}

pub fn parse_flags(
    package: &str,
    container: &str,
    declarations: Vec<Input>,
    values: Vec<Input>,
    mainline_beta_namespace_config: Option<PathBuf>,
    extended_permissions_options: ExtendedPermissionsOptions,
) -> Result<Vec<u8>> {
    let mut parsed_flags = ProtoParsedFlags::new();

    let beta_namespaces: Option<MainlineBetaNamespaces> = match mainline_beta_namespace_config {
        Some(file) => {
            let contents = std::fs::read_to_string(file)?;
            Some(serde_json::from_str(&contents)?)
        }
        None => None,
    };

    for mut input in declarations {
        let mut contents = String::new();
        input
            .reader
            .read_to_string(&mut contents)
            .with_context(|| format!("failed to read {}", input.source))?;

        let flag_declarations = aconfig_protos::flag_declarations::try_from_text_proto(&contents)
            .with_context(|| input.error_context())?;
        ensure!(
            package == flag_declarations.package(),
            "failed to parse {}: expected package {}, got {}",
            input.source,
            package,
            flag_declarations.package()
        );
        ensure!(
            container == flag_declarations.container(),
            "failed to parse {}: expected container {}, got {}",
            input.source,
            container,
            flag_declarations.container()
        );

        for mut flag_declaration in flag_declarations.flag.into_iter() {
            aconfig_protos::flag_declaration::verify_fields(&flag_declaration)
                .with_context(|| input.error_context())?;

            // create ParsedFlag using FlagDeclaration and default values
            let mut parsed_flag = ProtoParsedFlag::new();
            parsed_flag.set_container(container.to_string());
            parsed_flag.set_package(package.to_string());
            parsed_flag.set_name(flag_declaration.take_name());
            parsed_flag.set_namespace(flag_declaration.take_namespace());
            parsed_flag.set_description(flag_declaration.take_description());
            parsed_flag.bug.append(&mut flag_declaration.bug);
            parsed_flag.set_state(DEFAULT_FLAG_STATE);
            // for fixed read only or forced read only flags, set to read only.
            let flag_permission = if flag_declaration.is_fixed_read_only()
                || extended_permissions_options.force_read_only
            {
                ProtoFlagPermission::READ_ONLY
            } else {
                extended_permissions_options.default_permission
            };
            parsed_flag.set_permission(flag_permission);
            parsed_flag.set_is_fixed_read_only(flag_declaration.is_fixed_read_only());
            parsed_flag.set_is_exported(flag_declaration.is_exported());
            let mut tracepoint = ProtoTracepoint::new();
            tracepoint.set_source(input.source.clone());
            tracepoint.set_state(DEFAULT_FLAG_STATE);
            tracepoint.set_permission(flag_permission);
            parsed_flag.trace.push(tracepoint);

            let mut metadata = ProtoFlagMetadata::new();
            let purpose = flag_declaration.metadata.purpose();
            metadata.set_purpose(purpose);
            parsed_flag.metadata = Some(metadata).into();
            assign_storage_backend(&mut parsed_flag, &beta_namespaces)?;
            verify_mainline_beta_namespace_flag(&mut parsed_flag, &beta_namespaces)?;

            // verify ParsedFlag looks reasonable
            aconfig_protos::parsed_flag::verify_fields(&parsed_flag)?;

            // verify ParsedFlag can be added
            ensure!(
                parsed_flags.parsed_flag.iter().all(|other| other.name() != parsed_flag.name()),
                "failed to declare flag {} from {}: flag already declared",
                parsed_flag.name(),
                input.source
            );

            // add ParsedFlag to ParsedFlags
            parsed_flags.parsed_flag.push(parsed_flag);
        }
    }

    for mut input in values {
        let mut contents = String::new();
        input
            .reader
            .read_to_string(&mut contents)
            .with_context(|| format!("failed to read {}", input.source))?;
        let flag_values = aconfig_protos::flag_values::try_from_text_proto(&contents)
            .with_context(|| input.error_context())?;
        for mut flag_value in flag_values.flag_value.into_iter() {
            aconfig_protos::flag_value::verify_fields(&flag_value)
                .with_context(|| input.error_context())?;

            let Some(parsed_flag) = parsed_flags
                .parsed_flag
                .iter_mut()
                .find(|pf| pf.package() == flag_value.package() && pf.name() == flag_value.name())
            else {
                // (silently) skip unknown flags
                continue;
            };

            ensure!(
                !parsed_flag.is_fixed_read_only()
                    || flag_value.permission() == ProtoFlagPermission::READ_ONLY,
                "failed to set permission of flag {}, since this flag is fixed read only flag",
                flag_value.name()
            );
            if extended_permissions_options.force_read_only {
                flag_value.set_permission(ProtoFlagPermission::READ_ONLY);
            }

            parsed_flag.set_state(flag_value.state());
            if parsed_flag.permission() != flag_value.permission() {
                parsed_flag.set_permission(flag_value.permission());
                assign_storage_backend(parsed_flag, &beta_namespaces)?;
            }
            let mut tracepoint = ProtoTracepoint::new();
            tracepoint.set_source(input.source.clone());
            tracepoint.set_state(flag_value.state());
            tracepoint.set_permission(flag_value.permission());
            parsed_flag.trace.push(tracepoint);
        }
    }

    if !extended_permissions_options.allow_read_write {
        if let Some(pf) = parsed_flags
            .parsed_flag
            .iter()
            .find(|pf| pf.permission() == ProtoFlagPermission::READ_WRITE)
        {
            bail!("flag {} has permission READ_WRITE, but allow_read_write is false", pf.name());
        }
    }

    // Create a sorted parsed_flags
    aconfig_protos::parsed_flags::sort_parsed_flags(&mut parsed_flags);
    aconfig_protos::parsed_flags::verify_fields(&parsed_flags)?;
    let mut output = Vec::new();
    parsed_flags.write_to_vec(&mut output)?;
    Ok(output)
}

pub fn create_java_lib(
    mut input: Input,
    codegen_mode: CodegenMode,
    single_exported_file: bool,
    finalized_flags: FinalizedFlagMap,
) -> Result<Vec<OutputFile>> {
    let parsed_flags = input.try_parse_flags()?;
    let modified_parsed_flags =
        modify_parsed_flags_based_on_mode(parsed_flags.clone(), codegen_mode)?;
    let Some(package) = find_unique_package(&modified_parsed_flags) else {
        bail!("no parsed flags, or the parsed flags use different packages");
    };
    let package = package.to_string();
    let mut flag_names = extract_flag_names(parsed_flags)?;
    let package_fingerprint = compute_flags_fingerprint(&mut flag_names);
    let flag_ids = assign_flag_ids(&package, modified_parsed_flags.iter())?;
    let config = JavaCodegenConfig {
        codegen_mode,
        flag_ids,
        package_fingerprint,
        single_exported_file,
        finalized_flags,
        support_uau_annotation: !cfg!(enable_jarjar_flags_in_framwork),
    };
    generate_java_code(&package, modified_parsed_flags.into_iter(), config)
}

pub fn create_cpp_lib(mut input: Input, codegen_mode: CodegenMode) -> Result<Vec<OutputFile>> {
    // TODO(327420679): Enable export mode for native flag library
    ensure!(
        codegen_mode != CodegenMode::Exported,
        "Exported mode for generated c/c++ flag library is disabled"
    );
    let parsed_flags = input.try_parse_flags()?;
    let modified_parsed_flags =
        modify_parsed_flags_based_on_mode(parsed_flags.clone(), codegen_mode)?;
    let Some(package) = find_unique_package(&modified_parsed_flags) else {
        bail!("no parsed flags, or the parsed flags use different packages");
    };
    let package = package.to_string();
    let flag_ids = assign_flag_ids(&package, modified_parsed_flags.iter())?;
    let package_fingerprint: Option<u64> = if cfg!(enable_fingerprint_cpp) {
        let mut flag_names = extract_flag_names(parsed_flags)?;
        Some(compute_flags_fingerprint(&mut flag_names))
    } else {
        None
    };
    generate_cpp_code(
        &package,
        modified_parsed_flags.into_iter(),
        codegen_mode,
        flag_ids,
        package_fingerprint,
    )
}

pub fn create_rust_lib(mut input: Input, codegen_mode: CodegenMode) -> Result<OutputFile> {
    // // TODO(327420679): Enable export mode for native flag library
    ensure!(
        codegen_mode != CodegenMode::Exported,
        "Exported mode for generated rust flag library is disabled"
    );
    let parsed_flags = input.try_parse_flags()?;
    let modified_parsed_flags =
        modify_parsed_flags_based_on_mode(parsed_flags.clone(), codegen_mode)?;
    let Some(package) = find_unique_package(&modified_parsed_flags) else {
        bail!("no parsed flags, or the parsed flags use different packages");
    };
    let package = package.to_string();

    let package_fingerprint: Option<u64> = if cfg!(enable_fingerprint_rust) {
        let mut flag_names = extract_flag_names(parsed_flags)?;
        Some(compute_flags_fingerprint(&mut flag_names))
    } else {
        None
    };

    let flag_ids = assign_flag_ids(&package, modified_parsed_flags.iter())?;
    generate_rust_code(
        &package,
        flag_ids,
        modified_parsed_flags.into_iter(),
        codegen_mode,
        package_fingerprint,
    )
}

pub fn create_storage(
    caches: Vec<Input>,
    container: &str,
    file: &StorageFileType,
    version: u32,
) -> Result<Vec<u8>> {
    let parsed_flags_vec: Vec<ProtoParsedFlags> =
        caches.into_iter().map(|mut input| input.try_parse_flags()).collect::<Result<Vec<_>>>()?;
    generate_storage_file(container, parsed_flags_vec.iter(), file, version)
}

pub fn dump_parsed_flags(
    mut input: Vec<Input>,
    format: DumpFormat,
    filters: &[String],
    dedup: bool,
) -> Result<Vec<u8>> {
    let individually_parsed_flags: Result<Vec<ProtoParsedFlags>> =
        input.iter_mut().map(|i| i.try_parse_flags()).collect();
    let parsed_flags: ProtoParsedFlags =
        aconfig_protos::parsed_flags::merge(individually_parsed_flags?, dedup)?;
    let filters: Vec<Box<DumpPredicate>> = if filters.is_empty() {
        vec![Box::new(|_| true)]
    } else {
        filters
            .iter()
            .map(|f| crate::dump::create_filter_predicate(f))
            .collect::<Result<Vec<_>>>()?
    };
    crate::dump::dump_parsed_flags(
        parsed_flags.parsed_flag.into_iter().filter(|flag| filters.iter().any(|p| p(flag))),
        format,
    )
}

fn find_unique_package(parsed_flags: &[ProtoParsedFlag]) -> Option<&str> {
    let package = parsed_flags.first().map(|pf| pf.package())?;
    if parsed_flags.iter().any(|pf| pf.package() != package) {
        return None;
    }
    Some(package)
}

pub fn modify_parsed_flags_based_on_mode(
    parsed_flags: ProtoParsedFlags,
    codegen_mode: CodegenMode,
) -> Result<Vec<ProtoParsedFlag>> {
    fn exported_mode_flag_modifier(mut parsed_flag: ProtoParsedFlag) -> Result<ProtoParsedFlag> {
        parsed_flag.set_state(ProtoFlagState::DISABLED);
        parsed_flag.set_permission(ProtoFlagPermission::READ_WRITE);
        parsed_flag.set_is_fixed_read_only(false);
        let m = parsed_flag.metadata.as_mut().ok_or(anyhow!("missing metadata"))?;
        m.set_storage(ProtoFlagStorageBackend::ACONFIGD);
        Ok(parsed_flag)
    }

    fn force_read_only_mode_flag_modifier(
        mut parsed_flag: ProtoParsedFlag,
    ) -> Result<ProtoParsedFlag> {
        parsed_flag.set_permission(ProtoFlagPermission::READ_ONLY);
        let m = parsed_flag.metadata.as_mut().ok_or(anyhow!("missing metadata"))?;
        m.set_storage(ProtoFlagStorageBackend::NONE);
        Ok(parsed_flag)
    }

    let modified_parsed_flags: Vec<_> = match codegen_mode {
        CodegenMode::Exported => parsed_flags
            .parsed_flag
            .into_iter()
            .filter(|pf| pf.is_exported())
            .map(exported_mode_flag_modifier)
            .collect::<Result<Vec<_>>>()?,
        CodegenMode::ForceReadOnly => parsed_flags
            .parsed_flag
            .into_iter()
            .filter(|pf| !pf.is_exported())
            .map(force_read_only_mode_flag_modifier)
            .collect::<Result<Vec<_>>>()?,
        CodegenMode::Production | CodegenMode::Test => {
            parsed_flags.parsed_flag.into_iter().collect()
        }
    };
    if modified_parsed_flags.is_empty() {
        bail!("{codegen_mode} library contains no {codegen_mode} flags");
    }

    Ok(modified_parsed_flags)
}

pub fn assign_flag_ids<'a, I>(package: &str, parsed_flags_iter: I) -> Result<HashMap<String, u16>>
where
    I: Iterator<Item = &'a ProtoParsedFlag> + Clone,
{
    assert!(parsed_flags_iter.clone().tuple_windows().all(|(a, b)| a.name() <= b.name()));
    let mut flag_ids = HashMap::new();
    let mut flag_idx = 0;
    for pf in parsed_flags_iter {
        if package != pf.package() {
            return Err(anyhow::anyhow!("encountered a flag not in current package"));
        }

        // put a cap on how many flags a package can contain to 65534
        if flag_idx >= u16::MAX as u32 {
            return Err(anyhow::anyhow!("the number of flags in a package cannot exceed 65534"));
        }

        if should_include_flag(pf) {
            flag_ids.insert(pf.name().to_string(), flag_idx as u16);
            flag_idx += 1;
        }
    }
    Ok(flag_ids)
}

// Creates a fingerprint of the flag names (which requires sorting the vector).
// Fingerprint is used by both codegen and storage files.
pub fn compute_flags_fingerprint(flag_names: &mut Vec<String>) -> u64 {
    flag_names.sort();

    let mut hasher = SipHasher13::new();
    for flag in flag_names {
        hasher.write(flag.as_bytes());
    }
    hasher.finish()
}

// Converts ProtoParsedFlags into a vector of strings containing all of the flag
// names. Helper fn for creating fingerprint for codegen files. Flags must all
// belong to the same package.
fn extract_flag_names(flags: ProtoParsedFlags) -> Result<Vec<String>> {
    let separated_flags: Vec<ProtoParsedFlag> = flags.parsed_flag.into_iter().collect::<Vec<_>>();

    // All flags must belong to the same package as the fingerprint is per-package.
    let Some(_package) = find_unique_package(&separated_flags) else {
        bail!("No parsed flags, or the parsed flags use different packages.");
    };

    Ok(separated_flags
        .into_iter()
        .filter(should_include_flag)
        .map(|flag| flag.name.unwrap())
        .collect::<Vec<_>>())
}

// Check if a flag should be managed by aconfigd
pub fn should_include_flag(pf: &ProtoParsedFlag) -> bool {
    let is_platform_container = PLATFORM_CONTAINERS.iter().any(|&c| c == pf.container());
    let is_disabled_ro = pf.state == Some(ProtoFlagState::DISABLED.into())
        && pf.permission == Some(ProtoFlagPermission::READ_ONLY.into());

    !(is_platform_container && is_disabled_ro)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aconfig_protos::ProtoFlagPurpose;

    #[test]
    fn test_offset_fingerprint() {
        let parsed_flags = crate::test::parse_test_flags();
        let expected_fingerprint: u64 = 11551379960324242360;

        let mut extracted_flags = extract_flag_names(parsed_flags).unwrap();
        let hash_result = compute_flags_fingerprint(&mut extracted_flags);

        assert_eq!(hash_result, expected_fingerprint);
    }

    #[test]
    fn test_offset_fingerprint_matches_from_package() {
        let parsed_flags: ProtoParsedFlags = crate::test::parse_test_flags();

        // All test flags are in the same package, so fingerprint from all of them.
        let mut extracted_flags = extract_flag_names(parsed_flags.clone()).unwrap();
        let result_from_parsed_flags = compute_flags_fingerprint(&mut extracted_flags);

        let mut flag_names_vec = parsed_flags
            .parsed_flag
            .clone()
            .into_iter()
            .filter(should_include_flag)
            .map(|flag| flag.name.unwrap())
            .collect::<Vec<_>>();
        let result_from_names = compute_flags_fingerprint(&mut flag_names_vec);

        // Assert the same hash is generated for each case.
        assert_eq!(result_from_parsed_flags, result_from_names);
    }

    #[test]
    fn test_offset_fingerprint_different_packages_does_not_match() {
        // Parse flags from two packages.
        let parsed_flags: ProtoParsedFlags = crate::test::parse_test_flags();
        let second_parsed_flags = crate::test::parse_second_package_flags();

        let mut extracted_flags = extract_flag_names(parsed_flags).unwrap();
        let result_from_parsed_flags = compute_flags_fingerprint(&mut extracted_flags);
        let mut second_extracted_flags = extract_flag_names(second_parsed_flags).unwrap();
        let second_result = compute_flags_fingerprint(&mut second_extracted_flags);

        // Different flags should have a different fingerprint.
        assert_ne!(result_from_parsed_flags, second_result);
    }

    #[test]
    fn test_parse_flags() {
        let parsed_flags = crate::test::parse_test_flags(); // calls parse_flags
        aconfig_protos::parsed_flags::verify_fields(&parsed_flags).unwrap();

        let enabled_ro =
            parsed_flags.parsed_flag.iter().find(|pf| pf.name() == "enabled_ro").unwrap();
        assert!(aconfig_protos::parsed_flag::verify_fields(enabled_ro).is_ok());
        assert_eq!("com.android.aconfig.test", enabled_ro.package());
        assert_eq!("enabled_ro", enabled_ro.name());
        assert_eq!("This flag is ENABLED + READ_ONLY", enabled_ro.description());
        assert_eq!(ProtoFlagState::ENABLED, enabled_ro.state());
        assert_eq!(ProtoFlagPermission::READ_ONLY, enabled_ro.permission());
        assert_eq!(ProtoFlagPurpose::PURPOSE_BUGFIX, enabled_ro.metadata.purpose());
        assert_eq!(3, enabled_ro.trace.len());
        assert!(!enabled_ro.is_fixed_read_only());
        assert_eq!("tests/test.aconfig", enabled_ro.trace[0].source());
        assert_eq!(ProtoFlagState::DISABLED, enabled_ro.trace[0].state());
        assert_eq!(ProtoFlagPermission::READ_WRITE, enabled_ro.trace[0].permission());
        assert_eq!("tests/first.values", enabled_ro.trace[1].source());
        assert_eq!(ProtoFlagState::DISABLED, enabled_ro.trace[1].state());
        assert_eq!(ProtoFlagPermission::READ_WRITE, enabled_ro.trace[1].permission());
        assert_eq!("tests/second.values", enabled_ro.trace[2].source());
        assert_eq!(ProtoFlagState::ENABLED, enabled_ro.trace[2].state());
        assert_eq!(ProtoFlagPermission::READ_ONLY, enabled_ro.trace[2].permission());

        assert_eq!(9, parsed_flags.parsed_flag.len());
        for pf in parsed_flags.parsed_flag.iter() {
            if pf.name().starts_with("enabled_fixed_ro") {
                continue;
            }
            let first = pf.trace.first().unwrap();
            assert_eq!(DEFAULT_FLAG_STATE, first.state());
            assert_eq!(DEFAULT_FLAG_PERMISSION, first.permission());

            let last = pf.trace.last().unwrap();
            assert_eq!(pf.state(), last.state());
            assert_eq!(pf.permission(), last.permission());
        }

        let enabled_fixed_ro =
            parsed_flags.parsed_flag.iter().find(|pf| pf.name() == "enabled_fixed_ro").unwrap();
        assert!(enabled_fixed_ro.is_fixed_read_only());
        assert_eq!(ProtoFlagState::ENABLED, enabled_fixed_ro.state());
        assert_eq!(ProtoFlagPermission::READ_ONLY, enabled_fixed_ro.permission());
        assert_eq!(2, enabled_fixed_ro.trace.len());
        assert_eq!(ProtoFlagPermission::READ_ONLY, enabled_fixed_ro.trace[0].permission());
        assert_eq!(ProtoFlagPermission::READ_ONLY, enabled_fixed_ro.trace[1].permission());
    }

    #[test]
    fn test_parse_flags_setting_default() {
        let first_flag = r#"
        package: "com.first"
        container: "test"
        flag {
            name: "first"
            namespace: "first_ns"
            description: "This is the description of the first flag."
            bug: "123"
        }
        "#;
        let declaration =
            vec![Input { source: "momery".to_string(), reader: Box::new(first_flag.as_bytes()) }];
        let value: Vec<Input> = vec![];
        let extended_permissions_options = ExtendedPermissionsOptions {
            default_permission: ProtoFlagPermission::READ_ONLY,
            allow_read_write: true,
            force_read_only: false,
        };

        let flags_bytes = crate::commands::parse_flags(
            "com.first",
            "test",
            declaration,
            value,
            None,
            extended_permissions_options,
        )
        .unwrap();
        let parsed_flags =
            aconfig_protos::parsed_flags::try_from_binary_proto(&flags_bytes).unwrap();
        assert_eq!(1, parsed_flags.parsed_flag.len());
        let parsed_flag = parsed_flags.parsed_flag.first().unwrap();
        assert_eq!(ProtoFlagState::DISABLED, parsed_flag.state());
        assert_eq!(ProtoFlagPermission::READ_ONLY, parsed_flag.permission());
    }

    #[test]
    fn test_parse_flags_package_mismatch_between_declaration_and_command_line() {
        let first_flag = r#"
        package: "com.declaration.package"
        container: "first.container"
        flag {
            name: "first"
            namespace: "first_ns"
            description: "This is the description of the first flag."
            bug: "123"
        }
        "#;
        let declaration =
            vec![Input { source: "memory".to_string(), reader: Box::new(first_flag.as_bytes()) }];

        let value: Vec<Input> = vec![];
        let extended_permissions_options = ExtendedPermissionsOptions {
            default_permission: ProtoFlagPermission::READ_WRITE,
            allow_read_write: true,
            force_read_only: false,
        };

        let error = crate::commands::parse_flags(
            "com.argument.package",
            "first.container",
            declaration,
            value,
            None,
            extended_permissions_options,
        )
        .unwrap_err();
        assert_eq!(
            format!("{error:?}"),
            "failed to parse memory: expected package com.argument.package, got com.declaration.package"
        );
    }

    #[test]
    fn test_parse_flags_container_mismatch_between_declaration_and_command_line() {
        let first_flag = r#"
        package: "com.first"
        container: "declaration.container"
        flag {
            name: "first"
            namespace: "first_ns"
            description: "This is the description of the first flag."
            bug: "123"
        }
        "#;
        let declaration =
            vec![Input { source: "memory".to_string(), reader: Box::new(first_flag.as_bytes()) }];

        let value: Vec<Input> = vec![];
        let extended_permissions_options = ExtendedPermissionsOptions {
            default_permission: ProtoFlagPermission::READ_WRITE,
            allow_read_write: true,
            force_read_only: false,
        };

        let error = crate::commands::parse_flags(
            "com.first",
            "argument.container",
            declaration,
            value,
            None,
            extended_permissions_options,
        )
        .unwrap_err();
        assert_eq!(
            format!("{error:?}"),
            "failed to parse memory: expected container argument.container, got declaration.container"
        );
    }
    #[test]
    fn test_parse_flags_no_allow_read_write_default_error() {
        let first_flag = r#"
        package: "com.first"
        container: "com.first.container"
        flag {
            name: "first"
            namespace: "first_ns"
            description: "This is the description of the first flag."
            bug: "123"
        }
        "#;
        let declaration =
            vec![Input { source: "memory".to_string(), reader: Box::new(first_flag.as_bytes()) }];
        let extended_permissions_options = ExtendedPermissionsOptions {
            default_permission: ProtoFlagPermission::READ_WRITE,
            allow_read_write: false,
            force_read_only: false,
        };

        let error = crate::commands::parse_flags(
            "com.first",
            "com.first.container",
            declaration,
            vec![],
            None,
            extended_permissions_options,
        )
        .unwrap_err();
        assert_eq!(
            format!("{error:?}"),
            "flag first has permission READ_WRITE, but allow_read_write is false"
        );
    }

    #[test]
    fn test_parse_flags_no_allow_read_write_value_error() {
        let first_flag = r#"
        package: "com.first"
        container: "com.first.container"
        flag {
            name: "first"
            namespace: "first_ns"
            description: "This is the description of the first flag."
            bug: "123"
        }
        "#;
        let declaration =
            vec![Input { source: "memory".to_string(), reader: Box::new(first_flag.as_bytes()) }];

        let first_flag_value = r#"
        flag_value {
            package: "com.first"
            name: "first"
            state: DISABLED
            permission: READ_WRITE
        }
        "#;
        let value = vec![Input {
            source: "memory".to_string(),
            reader: Box::new(first_flag_value.as_bytes()),
        }];
        let extended_permissions_options = ExtendedPermissionsOptions {
            default_permission: ProtoFlagPermission::READ_ONLY,
            allow_read_write: false,
            force_read_only: false,
        };
        let error = crate::commands::parse_flags(
            "com.first",
            "com.first.container",
            declaration,
            value,
            None,
            extended_permissions_options,
        )
        .unwrap_err();
        assert_eq!(
            format!("{error:?}"),
            "flag first has permission READ_WRITE, but allow_read_write is false"
        );
    }

    #[test]
    fn test_parse_flags_no_allow_read_write_success() {
        let first_flag = r#"
        package: "com.first"
        container: "com.first.container"
        flag {
            name: "first"
            namespace: "first_ns"
            description: "This is the description of the first flag."
            bug: "123"
        }
        "#;
        let declaration =
            vec![Input { source: "memory".to_string(), reader: Box::new(first_flag.as_bytes()) }];

        let first_flag_value = r#"
        flag_value {
            package: "com.first"
            name: "first"
            state: DISABLED
            permission: READ_ONLY
        }
        "#;
        let value = vec![Input {
            source: "memory".to_string(),
            reader: Box::new(first_flag_value.as_bytes()),
        }];
        let extended_permissions_options = ExtendedPermissionsOptions {
            default_permission: ProtoFlagPermission::READ_ONLY,
            allow_read_write: false,
            force_read_only: false,
        };
        let flags_bytes = crate::commands::parse_flags(
            "com.first",
            "com.first.container",
            declaration,
            value,
            None,
            extended_permissions_options,
        )
        .unwrap();
        let parsed_flags =
            aconfig_protos::parsed_flags::try_from_binary_proto(&flags_bytes).unwrap();
        assert_eq!(1, parsed_flags.parsed_flag.len());
        let parsed_flag = parsed_flags.parsed_flag.first().unwrap();
        assert_eq!(ProtoFlagState::DISABLED, parsed_flag.state());
        assert_eq!(ProtoFlagPermission::READ_ONLY, parsed_flag.permission());
    }

    #[test]
    fn test_parse_flags_force_read_only_convert_read_write_to_read_only_success() {
        let first_flag = r#"
        package: "com.first"
        container: "com.first.container"
        flag {
            name: "first"
            namespace: "first_ns"
            description: "This is the description of the first flag."
            bug: "123"
        }
        "#;
        let declaration =
            vec![Input { source: "memory".to_string(), reader: Box::new(first_flag.as_bytes()) }];

        let first_flag_value = r#"
        flag_value {
            package: "com.first"
            name: "first"
            state: DISABLED
            permission: READ_WRITE
        }
        "#;
        let value = vec![Input {
            source: "memory".to_string(),
            reader: Box::new(first_flag_value.as_bytes()),
        }];
        let extended_permissions_options = ExtendedPermissionsOptions {
            default_permission: ProtoFlagPermission::READ_ONLY,
            allow_read_write: true,
            force_read_only: true,
        };
        let flags_bytes = crate::commands::parse_flags(
            "com.first",
            "com.first.container",
            declaration,
            value,
            None,
            extended_permissions_options,
        )
        .unwrap();
        let parsed_flags =
            aconfig_protos::parsed_flags::try_from_binary_proto(&flags_bytes).unwrap();
        assert_eq!(1, parsed_flags.parsed_flag.len());
        let parsed_flag = parsed_flags.parsed_flag.first().unwrap();
        assert_eq!(ProtoFlagState::DISABLED, parsed_flag.state());
        assert_eq!(ProtoFlagPermission::READ_ONLY, parsed_flag.permission());
    }

    #[test]
    fn test_parse_flags_force_read_only_no_allow_read_write_does_not_fail() {
        let first_flag = r#"
        package: "com.first"
        container: "com.first.container"
        flag {
            name: "first"
            namespace: "first_ns"
            description: "This is the description of the first flag."
            bug: "123"
        }
        "#;
        let declaration =
            vec![Input { source: "memory".to_string(), reader: Box::new(first_flag.as_bytes()) }];

        let first_flag_value = r#"
        flag_value {
            package: "com.first"
            name: "first"
            state: DISABLED
            permission: READ_WRITE
        }
        "#;
        let value = vec![Input {
            source: "memory".to_string(),
            reader: Box::new(first_flag_value.as_bytes()),
        }];
        let extended_permissions_options = ExtendedPermissionsOptions {
            default_permission: ProtoFlagPermission::READ_ONLY,
            allow_read_write: false,
            force_read_only: true,
        };
        let flags_bytes = crate::commands::parse_flags(
            "com.first",
            "com.first.container",
            declaration,
            value,
            None,
            extended_permissions_options,
        )
        .unwrap();
        let parsed_flags =
            aconfig_protos::parsed_flags::try_from_binary_proto(&flags_bytes).unwrap();
        assert_eq!(1, parsed_flags.parsed_flag.len());
        let parsed_flag = parsed_flags.parsed_flag.first().unwrap();
        assert_eq!(ProtoFlagState::DISABLED, parsed_flag.state());
        assert_eq!(ProtoFlagPermission::READ_ONLY, parsed_flag.permission());
    }

    #[test]
    fn test_parse_flags_override_fixed_read_only() {
        let first_flag = r#"
        package: "com.first"
        container: "com.first.container"
        flag {
            name: "first"
            namespace: "first_ns"
            description: "This is the description of the first flag."
            bug: "123"
            is_fixed_read_only: true
        }
        "#;
        let declaration =
            vec![Input { source: "memory".to_string(), reader: Box::new(first_flag.as_bytes()) }];

        let first_flag_value = r#"
        flag_value {
            package: "com.first"
            name: "first"
            state: DISABLED
            permission: READ_WRITE
        }
        "#;
        let value = vec![Input {
            source: "memory".to_string(),
            reader: Box::new(first_flag_value.as_bytes()),
        }];
        let extended_permissions_options = ExtendedPermissionsOptions {
            default_permission: ProtoFlagPermission::READ_WRITE,
            allow_read_write: true,
            force_read_only: false,
        };
        let error = crate::commands::parse_flags(
            "com.first",
            "com.first.container",
            declaration,
            value,
            None,
            extended_permissions_options,
        )
        .unwrap_err();
        assert_eq!(
            format!("{error:?}"),
            "failed to set permission of flag first, since this flag is fixed read only flag"
        );
    }

    #[test]
    fn test_parse_flags_metadata_purpose() {
        let metadata_flag = r#"
        package: "com.first"
        container: "test"
        flag {
            name: "first"
            namespace: "first_ns"
            description: "This is the description of this feature flag."
            bug: "123"
            metadata {
                purpose: PURPOSE_FEATURE
            }
        }
        "#;
        let declaration = vec![Input {
            source: "memory".to_string(),
            reader: Box::new(metadata_flag.as_bytes()),
        }];
        let value: Vec<Input> = vec![];
        let extended_permissions_options = ExtendedPermissionsOptions {
            default_permission: ProtoFlagPermission::READ_ONLY,
            allow_read_write: true,
            force_read_only: false,
        };
        let flags_bytes = crate::commands::parse_flags(
            "com.first",
            "test",
            declaration,
            value,
            None,
            extended_permissions_options,
        )
        .unwrap();
        let parsed_flags =
            aconfig_protos::parsed_flags::try_from_binary_proto(&flags_bytes).unwrap();
        assert_eq!(1, parsed_flags.parsed_flag.len());
        let parsed_flag = parsed_flags.parsed_flag.first().unwrap();
        assert_eq!(ProtoFlagPurpose::PURPOSE_FEATURE, parsed_flag.metadata.purpose());
    }

    fn get_parsed_flag_proto(
        container: &'static str,
        package: &'static str,
        decl: &'static str,
        val: Option<&'static str>,
        config: Option<PathBuf>,
    ) -> Result<ProtoParsedFlag> {
        let declaration =
            vec![Input { source: "memory".to_string(), reader: Box::new(decl.as_bytes()) }];

        let value: Vec<Input> = match val {
            Some(val_str) => {
                vec![Input { source: "memory".to_string(), reader: Box::new(val_str.as_bytes()) }]
            }
            None => {
                vec![]
            }
        };
        let extended_permissions_options = ExtendedPermissionsOptions {
            default_permission: ProtoFlagPermission::READ_WRITE,
            allow_read_write: true,
            force_read_only: false,
        };

        let flags_bytes = crate::commands::parse_flags(
            package,
            container,
            declaration,
            value,
            config,
            extended_permissions_options,
        )?;

        let parsed_flags = aconfig_protos::parsed_flags::try_from_binary_proto(&flags_bytes)?;

        assert_eq!(1, parsed_flags.parsed_flag.len());
        Ok(parsed_flags.parsed_flag.first().unwrap().clone())
    }

    #[test]
    fn test_parse_flags_mainline_beta_namespace_config() {
        let metadata_flag = r#"
        package: "com.first"
        container: "test"
        flag {
            name: "first"
            namespace: "first_ns"
            description: "This is the description of this feature flag."
            bug: "123"
        }
        "#;

        let config = Some(PathBuf::from("tests/mainline_beta_namespaces.json"));

        // Case 1, regular RW flag without value file override
        let parsed_flag =
            get_parsed_flag_proto("test", "com.first", metadata_flag, None, config.clone())
                .unwrap();
        assert_eq!(ProtoFlagStorageBackend::ACONFIGD, parsed_flag.metadata.storage());

        // Case 2, regular RW flag with value file override to RO
        let first_flag_value = r#"
        flag_value {
            package: "com.first"
            name: "first"
            state: DISABLED
            permission: READ_ONLY
        }
        "#;
        let parsed_flag = get_parsed_flag_proto(
            "test",
            "com.first",
            metadata_flag,
            Some(first_flag_value),
            config.clone(),
        )
        .unwrap();
        assert_eq!(ProtoFlagStorageBackend::NONE, parsed_flag.metadata.storage());

        // Case 3, fixed read only flag
        let metadata_flag = r#"
        package: "com.first"
        container: "test"
        flag {
            name: "first"
            namespace: "first_ns"
            description: "This is the description of this feature flag."
            bug: "123"
            is_fixed_read_only: true
        }
        "#;

        let parsed_flag =
            get_parsed_flag_proto("test", "com.first", metadata_flag, None, config.clone())
                .unwrap();
        assert_eq!(ProtoFlagStorageBackend::NONE, parsed_flag.metadata.storage());

        // Case 4, mainline beta namespace fixed read only flag
        let metadata_flag = r#"
        package: "com.first"
        container: "com.android.tethering"
        flag {
            name: "first"
            namespace: "com_android_tethering"
            description: "This is the description of this feature flag."
            bug: "123"
            is_fixed_read_only: true
        }
        "#;
        let parsed_flag = get_parsed_flag_proto(
            "com.android.tethering",
            "com.first",
            metadata_flag,
            None,
            config.clone(),
        )
        .unwrap();
        assert_eq!(ProtoFlagStorageBackend::NONE, parsed_flag.metadata.storage());

        // Case 5, mainline beta namespace platform flag
        let metadata_flag = r#"
        package: "com.first"
        container: "system"
        flag {
            name: "first"
            namespace: "com_android_tethering"
            description: "This is the description of this feature flag."
            bug: "123"
        }
        "#;
        let parsed_flag =
            get_parsed_flag_proto("system", "com.first", metadata_flag, None, config.clone())
                .unwrap();
        assert_eq!(ProtoFlagStorageBackend::ACONFIGD, parsed_flag.metadata.storage());

        // Case 6, mainline beta namespace mainline flag
        let metadata_flag = r#"
        package: "com.first"
        container: "com.android.tethering"
        flag {
            name: "first"
            namespace: "com_android_tethering"
            description: "This is the description of this feature flag."
            bug: "123"
        }
        "#;
        let parsed_flag = get_parsed_flag_proto(
            "com.android.tethering",
            "com.first",
            metadata_flag,
            None,
            config.clone(),
        )
        .unwrap();
        assert_eq!(ProtoFlagStorageBackend::DEVICE_CONFIG, parsed_flag.metadata.storage());

        // Case 7, mainline beta namespace mainline flag but without config
        let metadata_flag = r#"
        package: "com.first"
        container: "com.android.tethering"
        flag {
            name: "first"
            namespace: "com_android_tethering"
            description: "This is the description of this feature flag."
            bug: "123"
        }
        "#;
        let parsed_flag =
            get_parsed_flag_proto("com.android.tethering", "com.first", metadata_flag, None, None)
                .unwrap();
        assert_eq!(ProtoFlagStorageBackend::ACONFIGD, parsed_flag.metadata.storage());

        // Case 8, mainline beta namespace invalid container
        let metadata_flag = r#"
        package: "com.first"
        container: "com.android.tethering"
        flag {
            name: "first"
            namespace: "com_android_networkstack"
            description: "This is the description of this feature flag."
            bug: "123"
        }
        "#;
        let error = get_parsed_flag_proto(
            "com.android.tethering",
            "com.first",
            metadata_flag,
            None,
            config.clone(),
        )
        .unwrap_err();
        assert_eq!(
            format!("{error:?}"),
            "Creating com.android.tethering container flag in namespace com_android_networkstack is not allowed"
        );

        // Case 9, mainline beta namespace unsupported exported mode
        let metadata_flag = r#"
        package: "com.first"
        container: "com.android.networkstack"
        flag {
            name: "first"
            namespace: "com_android_networkstack"
            description: "This is the description of this feature flag."
            bug: "123"
            is_exported: true
        }
        "#;
        let error = get_parsed_flag_proto(
            "com.android.networkstack",
            "com.first",
            metadata_flag,
            None,
            config.clone(),
        )
        .unwrap_err();
        assert_eq!(
            format!("{error:?}"),
            "Creating exported flag com.first.first in namespace com_android_networkstack is not allowed"
        );
    }

    #[test]
    fn test_dump() {
        let input = parse_test_flags_as_input();
        let bytes = dump_parsed_flags(
            vec![input],
            DumpFormat::Custom("{fully_qualified_name}".to_string()),
            &[],
            false,
        )
        .unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains("com.android.aconfig.test.disabled_ro"));
    }

    #[test]
    fn test_dump_multiple_filters() {
        let input = parse_test_flags_as_input();
        let bytes = dump_parsed_flags(
            vec![input],
            DumpFormat::Custom("{fully_qualified_name}".to_string()),
            &[
                "container:system+state:ENABLED".to_string(),
                "container:system+permission:READ_WRITE".to_string(),
            ],
            false,
        )
        .unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        let expected_flag_list = &[
            "com.android.aconfig.test.disabled_rw",
            "com.android.aconfig.test.disabled_rw_exported",
            "com.android.aconfig.test.disabled_rw_in_other_namespace",
            "com.android.aconfig.test.enabled_fixed_ro",
            "com.android.aconfig.test.enabled_fixed_ro_exported",
            "com.android.aconfig.test.enabled_ro",
            "com.android.aconfig.test.enabled_ro_exported",
            "com.android.aconfig.test.enabled_rw",
        ];
        assert_eq!(expected_flag_list.map(|s| format!("{s}\n")).join(""), text);
    }

    #[test]
    fn test_dump_textproto_format_dedup() {
        let input = parse_test_flags_as_input();
        let input2 = parse_test_flags_as_input();
        let bytes =
            dump_parsed_flags(vec![input, input2], DumpFormat::Textproto, &[], true).unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert_eq!(
            None,
            crate::test::first_significant_code_diff(
                crate::test::TEST_FLAGS_TEXTPROTO.trim(),
                text.trim()
            )
        );
    }

    fn parse_test_flags_as_input() -> Input {
        let parsed_flags = crate::test::parse_test_flags();
        let binary_proto = parsed_flags.write_to_bytes().unwrap();
        let cursor = std::io::Cursor::new(binary_proto);
        let reader = Box::new(cursor);
        Input { source: "test.data".to_string(), reader }
    }

    #[test]
    fn test_modify_parsed_flags_based_on_mode_prod() {
        let parsed_flags = crate::test::parse_test_flags();
        let p_parsed_flags =
            modify_parsed_flags_based_on_mode(parsed_flags.clone(), CodegenMode::Production)
                .unwrap();
        assert_eq!(parsed_flags.parsed_flag.len(), p_parsed_flags.len());
        for (i, item) in p_parsed_flags.iter().enumerate() {
            assert!(parsed_flags.parsed_flag[i].eq(item));
        }
    }

    #[test]
    fn test_modify_parsed_flags_based_on_mode_exported() {
        let mut parsed_flags = crate::test::parse_test_flags();

        let pf = parsed_flags.parsed_flag.iter_mut().find(|pf| pf.is_exported()).unwrap();
        let m = pf.metadata.as_mut().unwrap();
        m.set_storage(ProtoFlagStorageBackend::DEVICE_CONFIG);

        let p_parsed_flags =
            modify_parsed_flags_based_on_mode(parsed_flags, CodegenMode::Exported).unwrap();
        assert_eq!(3, p_parsed_flags.len());
        for flag in p_parsed_flags.iter() {
            assert_eq!(ProtoFlagState::DISABLED, flag.state());
            assert_eq!(ProtoFlagPermission::READ_WRITE, flag.permission());
            assert_eq!(ProtoFlagStorageBackend::ACONFIGD, flag.metadata.storage());
            assert!(!flag.is_fixed_read_only());
            assert!(flag.is_exported());
        }

        let mut parsed_flags = crate::test::parse_test_flags();
        parsed_flags.parsed_flag.retain(|pf| !pf.is_exported());
        let error =
            modify_parsed_flags_based_on_mode(parsed_flags, CodegenMode::Exported).unwrap_err();
        assert_eq!("exported library contains no exported flags", format!("{error:?}"));
    }

    #[test]
    fn test_modify_parsed_flags_based_on_mode_forcereadonly() {
        let mut parsed_flags = crate::test::parse_test_flags();

        let pf = parsed_flags.parsed_flag.iter_mut().find(|pf| !pf.is_exported()).unwrap();
        let m = pf.metadata.as_mut().unwrap();
        m.set_storage(ProtoFlagStorageBackend::DEVICE_CONFIG);

        let p_parsed_flags =
            modify_parsed_flags_based_on_mode(parsed_flags, CodegenMode::ForceReadOnly).unwrap();
        assert_eq!(6, p_parsed_flags.len());
        for flag in p_parsed_flags.iter() {
            assert_eq!(ProtoFlagPermission::READ_ONLY, flag.permission());
            assert_eq!(ProtoFlagStorageBackend::NONE, flag.metadata.storage());
            assert!(!flag.is_exported());
        }
    }

    #[test]
    fn test_assign_flag_ids() {
        let mut parsed_flags = crate::test::parse_test_flags();
        let package = find_unique_package(&parsed_flags.parsed_flag).unwrap().to_string();
        let flag_ids = assign_flag_ids(&package, parsed_flags.parsed_flag.iter()).unwrap();
        let expected_flag_ids = HashMap::from([
            (String::from("disabled_rw"), 0_u16),
            (String::from("disabled_rw_exported"), 1_u16),
            (String::from("disabled_rw_in_other_namespace"), 2_u16),
            (String::from("enabled_fixed_ro"), 3_u16),
            (String::from("enabled_fixed_ro_exported"), 4_u16),
            (String::from("enabled_ro"), 5_u16),
            (String::from("enabled_ro_exported"), 6_u16),
            (String::from("enabled_rw"), 7_u16),
        ]);
        assert_eq!(flag_ids, expected_flag_ids);

        let pf = parsed_flags
            .parsed_flag
            .iter_mut()
            .find(|pf| pf.name() == "disabled_rw_in_other_namespace")
            .unwrap();
        let m = pf.metadata.as_mut().unwrap();
        m.set_storage(ProtoFlagStorageBackend::DEVICE_CONFIG);
        let flag_ids = assign_flag_ids(&package, parsed_flags.parsed_flag.iter()).unwrap();
        assert_eq!(flag_ids, expected_flag_ids);
    }

    #[test]
    fn test_modify_parsed_flags_based_on_mode_force_read_only() {
        let parsed_flags = crate::test::parse_test_flags();
        let p_parsed_flags =
            modify_parsed_flags_based_on_mode(parsed_flags.clone(), CodegenMode::ForceReadOnly)
                .unwrap();
        assert_eq!(6, p_parsed_flags.len());
        for pf in p_parsed_flags {
            assert_eq!(ProtoFlagPermission::READ_ONLY, pf.permission());
        }

        let mut parsed_flags = crate::test::parse_test_flags();
        parsed_flags.parsed_flag.retain_mut(|pf| pf.is_exported());
        let error = modify_parsed_flags_based_on_mode(parsed_flags, CodegenMode::ForceReadOnly)
            .unwrap_err();
        assert_eq!(
            "force-read-only library contains no force-read-only flags",
            format!("{error:?}")
        );
    }
}
