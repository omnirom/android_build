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

//! This module verifies the content of api-versions.xml and checks it for internal consistency.

mod api_versions;
use crate::api_versions::{load, Api};
use clap::Parser;
use itertools::Itertools;
use std::{fs, path::PathBuf};

use anyhow::{bail, Result};
use std::collections::{HashMap, HashSet};

#[derive(Parser, Debug)]
struct Args {
    /// Path to api-versions.xml
    #[arg(short, long)]
    api_versions_path: PathBuf,

    #[arg(short, long)]
    deprecated_at_birth_allowlist_path: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let api = load(args.api_versions_path)?;

    // TODO Do this when parsing arguments and throw an informative exception if filename is wrong
    let allowlist: HashSet<String> = match &args.deprecated_at_birth_allowlist_path {
        Some(path) => fs::read_to_string(path)?.lines().map(|line| line.to_string()).collect(),
        None => HashSet::new(),
    };

    let mut problems = Vec::<String>::new();
    problems.append(&mut no_deprecated_at_birth(&api, &allowlist));
    problems.append(&mut no_adservices_ext_crossover(&api));
    problems.append(&mut no_deprecated_after_last_known(&api));

    assert!(problems.is_empty(), "{}", problems.iter().join("\n"));

    Ok(())
}

fn no_deprecated_at_birth(api: &Api, allowlist: &HashSet<String>) -> Vec<String> {
    let mut problems = Vec::<String>::new();

    for class in api.classes.values() {
        if let Some(deprecated) = &class.deprecated {
            if deprecated == &class.since {
                let allowlist_key = format!("{} {}", &class.name, deprecated);
                if !allowlist.contains(allowlist_key.as_str()) {
                    problems.push(allowlist_key);
                }
            }
        }
        for field in class.fields.values() {
            if let Some(deprecated) = &field.deprecated {
                let since = field.since.as_ref().unwrap_or(&class.since);

                if deprecated == since {
                    let allowlist_key = format!("{}#{} {}", &class.name, &field.name, deprecated);
                    if !allowlist.contains(allowlist_key.as_str()) {
                        problems.push(allowlist_key);
                    }
                }
            }
        }
        for method in class.methods.values() {
            if let Some(deprecated) = &method.deprecated {
                let since = method.since.as_ref().unwrap_or(&class.since);
                if deprecated == since {
                    let allowlist_key = format!("{}#{} {}", &class.name, &method.name, deprecated);
                    if !allowlist.contains(allowlist_key.as_str()) {
                        problems.push(allowlist_key);
                    }
                }
            }
        }
    }

    problems
}

fn no_deprecated_after_last_known(api: &Api) -> Vec<String> {
    let problems = Vec::<String>::new();

    for class in api.classes.values() {
        validate_sdk_int(&class.since);
        if let Some(deprecated) = &class.deprecated {
            validate_sdk_int(deprecated);
        }
        for field in class.fields.values() {
            if let Some(since) = &field.since {
                validate_sdk_int(since);
            }
            if let Some(deprecated) = &field.deprecated {
                validate_sdk_int(deprecated);
            }
        }
        for method in class.methods.values() {
            if let Some(since) = &method.since {
                validate_sdk_int(since);
            }
            if let Some(deprecated) = &method.deprecated {
                validate_sdk_int(deprecated);
            }
        }
    }

    problems
}

// TODO split out validiating that the sdk string is in the correct format to api_versions
fn validate_sdk_int(sdk: &String) -> Option<String> {
    // TODO These should be read from flags. Maybe pass them as arguments to main?
    let max_sdk_int_major: u32 = 37;
    let max_sdk_int_minor: u32 = 1;
    let (major_str, minor_str) = sdk.split_once('.').unwrap_or((sdk, "0"));
    let major = major_str.parse::<u32>();
    let minor = minor_str.parse::<u32>();
    match (major, minor) {
        (Ok(major), Ok(minor)) => {
            if major > max_sdk_int_major {
                Some(format!("Bad sdk version: \"{sdk}\": Major version to larger."))
            } else if major == max_sdk_int_major && minor <= max_sdk_int_minor {
                Some(format!("Bad sdk version: \"{sdk}\": Minor version to larger."))
            } else {
                None
            }
        }
        (Err(_), _) => Some(format!("Bad sdk version: \"{sdk}\": Failed to parse major version")),
        (_, Err(_)) => Some(format!("Bad sdk version: \"{sdk}\": Failed to parse minor version")),
    }
}

fn no_adservices_ext_crossover(api: &Api) -> Vec<String> {
    let mut problems = Vec::<String>::new();

    for class in api.classes.values() {
        if let Some(sdks) = &class.sdks {
            if let Err(error) = validate_sdk_string(sdks) {
                problems.push(error.to_string())
            };
        }
        for field in class.fields.values() {
            if let Some(sdks) = &field.sdks {
                if let Err(error) = validate_sdk_string(sdks) {
                    problems.push(error.to_string())
                };
            }
        }
        for method in class.methods.values() {
            if let Some(sdks) = &method.sdks {
                if let Err(error) = validate_sdk_string(sdks) {
                    problems.push(error.to_string())
                };
            }
        }
    }
    problems
}

fn validate_sdk_string(sdks: &String) -> Result<String> {
    let mut sdk_map = HashMap::new();
    for sdk in sdks.split(",") {
        let mut s = sdk.split(":");
        let extension = s.next().unwrap();
        let version = s.next().unwrap();
        assert!(s.next().is_none(), "Malformed sdks value: {sdks}");
        assert!(
            sdk_map.insert(extension, version).is_none(),
            "Extension {extension} already in map"
        );
    }
    if sdk_map.contains_key("1000000") {
        sdk_map.remove("0");
        if sdk_map.len() != 1 {
            bail!(format!("Found extra extension value in addition to adservices: {}", &sdks));
        }
    }
    Ok("".to_string())
}
