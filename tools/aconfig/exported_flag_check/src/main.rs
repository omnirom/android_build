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

//! `exported-flag-check` is a tool to ensures that exported flags are used as intended
use anyhow::{anyhow, bail, ensure, Context, Result};
use clap::{builder::ArgAction, Arg, ArgMatches, Command};
use std::io::Write;
use std::{collections::HashSet, fs, fs::File, io::Read, path::PathBuf};

mod utils;

use utils::{
    check_all_exported_flags, extract_flagged_api_flags, filter_api_flags,
    get_exported_flags_from_binary_proto, read_flag_from_binary, FlagId,
};

const HELP: &str = "Check Exported Flags

This tool ensures that exported flags are used as intended. Exported flags, marked with
`is_exported: true` in their declaration, are designed to control access to specific API
features. This tool identifies and reports any exported flags that are not currently
associated with an API feature, preventing unnecessary flag proliferation and maintaining
a clear API design.

Commands:

This tool offers two commands:

1. validate-exported-flags :This command verifies that all exported flags within 
   the current source tree are actively used to guard API features.

Arguments:
    --parsed-flags-file: Current aconfig flag values from source tree
    --api-signature-file: API signature files from source tree (*current.txt files)
    --finalized-flags-file: The previous finalized-flags.txt files from prebuilts/sdk

Example:
exported-flag-tool validate-exported-flags \
    --parsed-flags-file out/soong/aconfig/parsed_flags.pb \
    --api-signature-file frameworks/base/api/current.txt \
    --api-signature-file external/library/api/current.txt \
    --finalized-flags-file prebuilts/sdk/34/public/finalized-flags.txt

2. filter-api-flags: This command processes an input list of flags and filters it, based on
   the non-api lists to produce an output file containing only the exported flags that
   are used for controlling API features.

Arguments:
    --cache: The path to the input aconfig flag proto file
    --out: The output file

Example:
exported-flag-tool filter-api-flags \
    --cache build/intermediate/foo_flags.pb \
    --cache build/intermediate/bar_flags.pb \
    --out build/intermediate/api_relevant_exported_flags.pb

";

fn cli() -> Command {
    Command::new("exported-flag-check")
        .subcommand_required(true)
        .subcommand(
            Command::new("validate-exported-flags")
                .arg(Arg::new("parsed-flags-file").long("parsed-flags-file").required(true))
                .arg(
                    Arg::new("api-signature-file")
                        .long("api-signature-file")
                        .required(true)
                        .action(ArgAction::Append),
                )
                .arg(Arg::new("finalized-flags-file").long("finalized-flags-file").required(true)),
        )
        .subcommand(
            Command::new("filter-api-flags")
                .arg(Arg::new("cache").long("cache").required(true))
                .arg(Arg::new("out").long("out").required(true)),
        )
        .after_help(HELP.trim())
}

fn open_single_file(matches: &ArgMatches, arg_name: &str) -> Result<Box<dyn Read>> {
    let Some(path) = matches.get_one::<String>(arg_name) else {
        bail!("missing argument {}", arg_name);
    };
    Ok(Box::new(File::open(path)?))
}

fn open_multiple_files(matches: &ArgMatches, arg_name: &str) -> Result<Vec<Box<dyn Read>>> {
    let mut opened_files: Vec<Box<dyn Read>> = Vec::new();
    for path in matches.get_many::<String>(arg_name).unwrap_or_default() {
        opened_files.push(Box::new(File::open(path)?));
    }
    Ok(opened_files)
}

fn validate_exported_flags<R: Read>(
    parsed_flags_file: R,
    api_signature_files: Vec<R>,
    finalized_flags_file: R,
    non_api_flags: R,
    allow_flag_package: R,
) -> Result<Vec<FlagId>> {
    let mut flags_used_with_flaggedapi_annotation = HashSet::new();
    for file in api_signature_files {
        let flags = extract_flagged_api_flags(file)?;
        flags_used_with_flaggedapi_annotation.extend(flags);
    }
    let all_flags = get_exported_flags_from_binary_proto(parsed_flags_file)?;
    let already_finalized_flags = read_flag_from_binary(finalized_flags_file)?;
    let allow_flag_set = read_flag_from_binary(non_api_flags)?;
    let allow_package_set = read_flag_from_binary(allow_flag_package)?;

    let exported_flags = check_all_exported_flags(
        &flags_used_with_flaggedapi_annotation,
        &all_flags,
        &already_finalized_flags,
        &allow_flag_set,
        &allow_package_set,
    )?;

    println!("{}", exported_flags.join("\n"));

    Ok(exported_flags)
}

fn main() -> Result<()> {
    let matches = cli().get_matches();
    match matches.subcommand() {
        Some(("validate-exported-flags", sub_matches)) => {
            let parsed_flags_file = open_single_file(sub_matches, "parsed-flags-file")?;
            let api_signature_files = open_multiple_files(sub_matches, "api-signature-file")?;
            let finalized_flags_file = open_single_file(sub_matches, "finalized-flags-file")?;
            let non_api_flags = include_str!("../non_api_flags_list.txt");
            let non_api_flags_packages = include_str!("../non_api_flags_packages.txt");

            let exported_flags = validate_exported_flags(
                parsed_flags_file,
                api_signature_files,
                finalized_flags_file,
                Box::new(non_api_flags.as_bytes()),
                Box::new(non_api_flags_packages.as_bytes()),
            )?;

            ensure!(
                exported_flags.is_empty(),
                "Flags {} are exported but not used to guard any API. \
            Exported flag should be used to guard API",
                exported_flags.join(",")
            );
        }
        Some(("filter-api-flags", sub_matches)) => {
            let cache = open_single_file(sub_matches, "cache")?;

            let Some(out_file_arg) = sub_matches.get_one::<String>("out") else {
                bail!("argument out is missing");
            };
            let out_file = PathBuf::from(out_file_arg);
            let mut non_api_flags_set =
                read_flag_from_binary(&include_bytes!("../non_api_flags_list.txt")[..])?;
            let skip_flags_set =
                read_flag_from_binary(&include_bytes!("../skip_api_filter_list.txt")[..])?;
            skip_flags_set.iter().for_each(|flag| {
                non_api_flags_set.remove(flag);
            });
            let filtered_cache = filter_api_flags(cache, &non_api_flags_set)?;
            let parent = out_file
                .parent()
                .ok_or(anyhow!("unable to locate parent of output file {}", out_file.display()))?;
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
            let mut file = fs::File::create(&out_file)
                .with_context(|| format!("failed to open {}", out_file.display()))?;
            file.write_all(&filtered_cache)
                .with_context(|| format!("failed to write to {}", out_file.display()))?;
        }
        _ => unreachable!(),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aconfig_protos::parsed_flags;
    use protobuf::Message;

    #[test]
    fn test() {
        let input = std::str::from_utf8(include_bytes!("../tests/flags.textproto")).unwrap();
        let parsed_flags = parsed_flags::try_from_text_proto(input).unwrap();
        let mut all_flags_to_be_finalized = Vec::new();
        parsed_flags.write_to_vec(&mut all_flags_to_be_finalized).unwrap();
        let flags_used_with_flaggedapi_annotation =
            vec![&include_bytes!("../tests/api-signature-file.txt")[..]];
        let already_finalized_flags = include_bytes!("../tests/finalized-flags.txt");
        let non_api_flags = "record_finalized_flags.test.boo".as_bytes();
        let allow_flag_package = "".as_bytes();

        let exported_flags = validate_exported_flags(
            &all_flags_to_be_finalized[..],
            flags_used_with_flaggedapi_annotation,
            &already_finalized_flags[..],
            non_api_flags,
            allow_flag_package,
        )
        .unwrap();

        assert_eq!(1, exported_flags.len());
        assert_eq!("record_finalized_flags.test.not_enabled", exported_flags[0]);
    }
}
