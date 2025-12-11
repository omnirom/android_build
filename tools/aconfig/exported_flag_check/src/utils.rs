/*
 * Copyright (C) 2025 The Android Open Source Project
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

use aconfig_protos::{ParsedFlagExt, ProtoParsedFlags};
use anyhow::{anyhow, Result};
use protobuf::Message;
use regex::Regex;
use std::{
    collections::HashSet,
    io::{BufRead, BufReader, Read},
};

pub(crate) type FlagId = String;

/// Grep for all flags used with @FlaggedApi annotations in an API signature file (*current.txt
/// file).
pub(crate) fn extract_flagged_api_flags<R: Read>(mut reader: R) -> Result<HashSet<FlagId>> {
    let mut haystack = String::new();
    reader.read_to_string(&mut haystack)?;
    let regex = Regex::new(r#"(?ms)@FlaggedApi\("(.*?)"\)"#).unwrap();
    let iter = regex.captures_iter(&haystack).map(|cap| cap[1].to_owned());
    Ok(HashSet::from_iter(iter))
}

/// Read a list of flag names. The input is expected to be plain text, with each line containing
/// the name of a single flag.
pub(crate) fn read_flag_from_binary<R: Read>(reader: R) -> Result<HashSet<FlagId>> {
    Ok(BufReader::new(reader)
        .lines()
        .map_while(Result::ok) // Ignore lines that fail to read
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("/"))
        .collect())
}

/// Parse a ProtoParsedFlags binary protobuf blob and return the fully qualified names of flags
/// have is_exported as true.
pub(crate) fn get_exported_flags_from_binary_proto<R: Read>(
    mut reader: R,
) -> Result<HashSet<FlagId>> {
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer)?;
    let parsed_flags = aconfig_protos::parsed_flags::try_from_binary_proto(&buffer)
        .map_err(|_| anyhow!("failed to parse binary proto"))?;
    let iter = parsed_flags
        .parsed_flag
        .into_iter()
        .filter(|flag| flag.is_exported())
        .map(|flag| flag.fully_qualified_name());
    Ok(HashSet::from_iter(iter))
}

/// Filter out the flags have is_exported as true but not used with @FlaggedApi annotations
/// in the source tree, or in the previously finalized flags set.
pub(crate) fn check_all_exported_flags(
    flags_used_with_flaggedapi_annotation: &HashSet<FlagId>,
    all_flags: &HashSet<FlagId>,
    already_finalized_flags: &HashSet<FlagId>,
    allow_flag_set: &HashSet<FlagId>,
    allow_package_set: &HashSet<FlagId>,
) -> Result<Vec<FlagId>> {
    let new_flags: Vec<FlagId> = all_flags
        .difference(flags_used_with_flaggedapi_annotation)
        .cloned()
        .collect::<HashSet<_>>()
        .difference(already_finalized_flags)
        .cloned()
        .collect::<HashSet<_>>()
        .difference(allow_flag_set)
        .filter(|flag| {
            if let Some(last_dot_index) = flag.rfind('.') {
                let package_name = &flag[..last_dot_index];
                !allow_package_set.contains(package_name)
            } else {
                true
            }
        })
        .cloned()
        .collect();

    Ok(new_flags)
}

pub(crate) fn filter_api_flags<R: Read>(
    mut cache: R,
    non_api_flag_set: &HashSet<FlagId>,
) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    cache.read_to_end(&mut buffer)?;
    let parsed_flags = aconfig_protos::parsed_flags::try_from_binary_proto(&buffer)
        .map_err(|_| anyhow!("failed to parse binary proto"))?;
    let mut filtered_parsed_flags = ProtoParsedFlags::new();
    parsed_flags
        .parsed_flag
        .into_iter()
        .filter(|flag| {
            flag.is_exported() && !non_api_flag_set.contains(&flag.fully_qualified_name())
        })
        .for_each(|flag| filtered_parsed_flags.parsed_flag.push(flag.clone()));
    aconfig_protos::parsed_flags::sort_parsed_flags(&mut filtered_parsed_flags);
    let mut output = Vec::new();
    filtered_parsed_flags.write_to_vec(&mut output)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aconfig_protos::parsed_flags;

    #[test]
    fn test_extract_flagged_api_flags() {
        let api_signature_files = include_bytes!("../tests/api-signature-file.txt");
        let flags = extract_flagged_api_flags(&api_signature_files[..]).unwrap();
        assert_eq!(
            flags,
            HashSet::from_iter(vec![
                "record_finalized_flags.test.foo".to_string(),
                "this.flag.is.not.used".to_string(),
            ])
        );
    }

    #[test]
    fn test_read_finalized_flags() {
        let input = include_bytes!("../tests/finalized-flags.txt");
        let flags = read_flag_from_binary(&input[..]).unwrap();
        assert_eq!(
            flags,
            HashSet::from_iter(vec![
                "record_finalized_flags.test.bar".to_string(),
                "record_finalized_flags.test.baz".to_string(),
            ])
        );
    }

    #[test]
    fn test_get_exported_flags_from_binary_proto() {
        let input = std::str::from_utf8(include_bytes!("../tests/flags.textproto")).unwrap();
        let parsed_flags = parsed_flags::try_from_text_proto(input).unwrap();
        let mut bytes = Vec::new();
        parsed_flags.write_to_vec(&mut bytes).unwrap();
        let flags = get_exported_flags_from_binary_proto(&bytes[..]).unwrap();
        assert_eq!(
            flags,
            HashSet::from_iter(vec![
                "record_finalized_flags.test.foo".to_string(),
                "record_finalized_flags.test.not_enabled".to_string(),
                "record_finalized_flags.test.bar".to_string(),
                "record_finalized_flags.test.boo".to_string(),
            ])
        );
    }

    #[test]
    fn test_filter_api_flags() {
        let input = std::str::from_utf8(include_bytes!("../tests/flags.textproto")).unwrap();
        let parsed_flags = parsed_flags::try_from_text_proto(input).unwrap();
        let mut bytes = Vec::new();
        parsed_flags.write_to_vec(&mut bytes).unwrap();
        let allow_flag_file = r#"
        record_finalized_flags.test.boo
        record_finalized_flags.test.not_enabled
        "#
        .as_bytes();

        let allow_flag_set = read_flag_from_binary(allow_flag_file).unwrap();
        let flags = filter_api_flags(&bytes[..], &allow_flag_set).unwrap();
        let parsed_flags = aconfig_protos::parsed_flags::try_from_binary_proto(&flags).unwrap();
        assert_eq!(2, parsed_flags.parsed_flag.len());

        let ret = parsed_flags
            .parsed_flag
            .into_iter()
            .filter(|flag| flag.is_exported())
            .map(|flag| flag.fully_qualified_name())
            .collect::<HashSet<FlagId>>();
        assert_eq!(
            ret,
            HashSet::from_iter(vec![
                "record_finalized_flags.test.foo".to_string(),
                "record_finalized_flags.test.bar".to_string(),
            ])
        );

        let allow_flag_file = r#"
        record_finalized_flags.test.foo
        record_finalized_flags.test.boo
        record_finalized_flags.test.not_enabled
        "#
        .as_bytes();
        let allow_flag_set = read_flag_from_binary(allow_flag_file).unwrap();
        let flags = filter_api_flags(&bytes[..], &allow_flag_set).unwrap();
        let parsed_flags = aconfig_protos::parsed_flags::try_from_binary_proto(&flags).unwrap();
        assert_eq!(1, parsed_flags.parsed_flag.len());

        let ret = parsed_flags
            .parsed_flag
            .into_iter()
            .filter(|flag| flag.is_exported())
            .map(|flag| flag.fully_qualified_name())
            .collect::<HashSet<FlagId>>();
        assert_eq!(ret, HashSet::from_iter(vec!["record_finalized_flags.test.bar".to_string(),]));
    }

    #[test]
    fn test_read_flag_from_binary() {
        let test_binary_file = r#"
        // This is a comment
        //record_finalized_flags.test.not_enabled
        record_finalized_flags.test.bar

        record_finalized_flags.test.baz
        "#
        .as_bytes();
        let ret = read_flag_from_binary(test_binary_file).unwrap();
        assert_eq!(2, ret.len());
        assert_eq!(
            ret,
            HashSet::from_iter(vec![
                "record_finalized_flags.test.bar".to_string(),
                "record_finalized_flags.test.baz".to_string(),
            ])
        );
    }
}
