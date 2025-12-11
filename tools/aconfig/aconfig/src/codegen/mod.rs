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

pub mod cpp;
pub mod java;
pub mod rust;

use crate::commands::should_include_flag;
use aconfig_protos::{is_valid_name_ident, is_valid_package_ident};
use aconfig_protos::{ParsedFlagExt, ProtoParsedFlag};
use anyhow::{ensure, Result};
use clap::ValueEnum;
use std::collections::HashMap;

pub fn create_device_config_ident(package: &str, flag_name: &str) -> Result<String> {
    ensure!(is_valid_package_ident(package), "bad package");
    ensure!(is_valid_name_ident(flag_name), "bad flag name");
    Ok(format!("{package}.{flag_name}"))
}

pub(crate) fn get_flag_offset_in_storage_file(
    flag_ids: &HashMap<String, u16>,
    pf: &ProtoParsedFlag,
) -> Result<u16> {
    match flag_ids.get(pf.name()) {
        Some(offset) => {
            ensure!(
                should_include_flag(pf),
                "flag {} should not have an assigned flag id in new storage file",
                pf.fully_qualified_name()
            );
            Ok(*offset)
        }
        None => {
            ensure!(
                !should_include_flag(pf),
                "flag {} should have an assigned flag id in new storage file",
                pf.fully_qualified_name()
            );
            Ok(u16::MAX)
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum CodegenMode {
    Exported,
    ForceReadOnly,
    Production,
    Test,
}

impl std::fmt::Display for CodegenMode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            CodegenMode::Exported => write!(f, "exported"),
            CodegenMode::ForceReadOnly => write!(f, "force-read-only"),
            CodegenMode::Production => write!(f, "production"),
            CodegenMode::Test => write!(f, "test"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aconfig_protos::ProtoFlagPermission;

    #[test]
    fn test_create_device_config_ident() {
        assert_eq!(
            "com.foo.bar.some_flag",
            create_device_config_ident("com.foo.bar", "some_flag").unwrap()
        );
    }

    #[test]
    fn test_get_flag_offset_in_storage_file() {
        let mut parsed_flags = crate::test::parse_test_flags();
        let pf = parsed_flags.parsed_flag.iter_mut().find(|pf| pf.name() == "disabled_rw").unwrap();
        let flag_ids = HashMap::from([(String::from("disabled_rw"), 0_u16)]);

        assert_eq!(0_u16, get_flag_offset_in_storage_file(&flag_ids, pf).unwrap());

        pf.set_permission(ProtoFlagPermission::READ_ONLY);
        let error = get_flag_offset_in_storage_file(&flag_ids, pf).unwrap_err();
        assert_eq!(
            format!("{error:?}"),
            "flag com.android.aconfig.test.disabled_rw should not have an assigned flag id in new storage file"
        );

        pf.set_name(String::from("enabled_rw"));
        assert_eq!(u16::MAX, get_flag_offset_in_storage_file(&flag_ids, pf).unwrap());

        pf.set_permission(ProtoFlagPermission::READ_WRITE);
        let error = get_flag_offset_in_storage_file(&flag_ids, pf).unwrap_err();
        assert_eq!(
            format!("{error:?}"),
            "flag com.android.aconfig.test.enabled_rw should have an assigned flag id in new storage file"
        );
    }
}
