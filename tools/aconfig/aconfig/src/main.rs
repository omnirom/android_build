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

//! `aconfig` is a build time tool to manage build time configurations, such as feature flags.

mod cli_parser;
mod codegen;
mod commands;
mod dump;
mod storage;

use commands::Input;
use convert_finalized_flags::FinalizedFlagMap;

use anyhow::{anyhow, Context, Result};
use std::env;
use std::fs;
use std::fs::File;
use std::io;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

#[cfg(test)]
mod test;

fn load_finalized_flags() -> Result<FinalizedFlagMap> {
    let json_str = include_str!(concat!(env!("OUT_DIR"), "/finalized_flags_record.json"));
    let map = serde_json::from_str(json_str)?;
    Ok(map)
}

fn open_zero_or_more_files(file_paths: &Vec<String>) -> Result<Vec<Input>> {
    let mut opened_files = vec![];
    for path in file_paths {
        let file = Box::new(File::open(path).with_context(|| format!("Couldn't open {path}"))?);
        opened_files.push(Input { source: path.to_string(), reader: file });
    }
    Ok(opened_files)
}

fn open_single_file(path: &str) -> Result<Input> {
    let file = Box::new(File::open(path).with_context(|| format!("Couldn't open {path}"))?);
    Ok(Input { source: path.to_string(), reader: file })
}

fn write_output_files_relative_to_dir(
    root: &Path,
    output_files: &[commands::OutputFile],
) -> Result<()> {
    for output_file in output_files {
        let path = root.join(&output_file.path);
        let parent = path
            .parent()
            .ok_or_else(|| anyhow!("unable to locate parent of output file {}", path.display()))?;
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
        let mut file = fs::File::create(&path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        file.write_all(&output_file.contents)
            .with_context(|| format!("failed to write to {}", path.display()))?;
    }
    Ok(())
}

fn write_output_to_file_or_stdout(path: &str, data: &[u8]) -> Result<()> {
    if path == "-" {
        io::stdout().write_all(data).context("failed to write to stdout")?;
    } else {
        fs::File::create(path)
            .with_context(|| format!("failed to open {path}"))?
            .write_all(data)
            .with_context(|| format!("failed to write to {path}"))?;
    }
    Ok(())
}

struct RealResponseFileReader;

impl cli_parser::ResponseFileReader for RealResponseFileReader {
    fn read_to_bufread(&self, path_str: &str) -> Result<Box<dyn BufRead>> {
        let path = Path::new(path_str);
        let file = File::open(path)
            .with_context(|| format!("Failed to open response file: {}", path.display()))?;
        let reader = BufReader::new(file);
        Ok(Box::new(reader))
    }
}

fn main() -> Result<()> {
    let reader = RealResponseFileReader;
    let processed_args = cli_parser::process_raw_args(env::args_os(), &reader)?;
    let parsed_command = cli_parser::parse_args(processed_args)?;

    match parsed_command {
        cli_parser::ParsedCommand::CreateCache {
            package,
            container,
            declarations,
            values,
            default_permission,
            allow_read_write,
            cache_out_path,
            mainline_beta_namespace_config,
            force_read_only,
        } => {
            let extended_permissions_options = commands::ExtendedPermissionsOptions {
                default_permission,
                allow_read_write,
                force_read_only,
            };
            let output = commands::parse_flags(
                &package,
                &container,
                open_zero_or_more_files(&declarations)?, // declarations
                open_zero_or_more_files(&values)?,       // values
                mainline_beta_namespace_config,
                extended_permissions_options,
            )
            .context("failed to create cache")?;
            write_output_to_file_or_stdout(&cache_out_path, &output)?;
        }
        cli_parser::ParsedCommand::CreateJavaLib {
            cache_path,
            out_dir,
            mode,
            single_exported_file,
        } => {
            let finalized_flags = load_finalized_flags()?;
            let generated_files = commands::create_java_lib(
                open_single_file(&cache_path)?, // cache
                mode,
                single_exported_file,
                finalized_flags,
            )
            .context("failed to create java lib")?;
            write_output_files_relative_to_dir(&out_dir, &generated_files)?;
        }
        cli_parser::ParsedCommand::CreateCppLib { cache_path, out_dir, mode } => {
            let generated_files = commands::create_cpp_lib(
                open_single_file(&cache_path)?, // cache,
                mode,
            )
            .context("failed to create cpp lib")?;
            write_output_files_relative_to_dir(&out_dir, &generated_files)?;
        }
        cli_parser::ParsedCommand::CreateRustLib { cache_path, out_dir, mode } => {
            let generated_file = commands::create_rust_lib(
                open_single_file(&cache_path)?, // cach
                mode,
            )
            .context("failed to create rust lib")?;
            write_output_files_relative_to_dir(&out_dir, &[generated_file])?;
        }
        cli_parser::ParsedCommand::DumpCache { cache_paths, format, filters, dedup, out_path } => {
            let output = commands::dump_parsed_flags(
                open_zero_or_more_files(&cache_paths)?,
                format,
                &filters,
                dedup,
            )?;
            write_output_to_file_or_stdout(&out_path, &output)?;
        }
        cli_parser::ParsedCommand::CreateStorage {
            container,
            file_type,
            cache_paths,
            out_path,
            version,
        } => {
            let output = commands::create_storage(
                open_zero_or_more_files(&cache_paths)?,
                &container,
                &file_type,
                version,
            )
            .context("failed to create storage files")?;
            write_output_to_file_or_stdout(&out_path, &output)?;
        }
    }

    Ok(())
}
