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

use all_release_configs_proto::build_flags_out::{ReleaseConfigArtifact, ReleaseConfigsArtifact};
use protobuf::Message;
use std::collections::HashMap;
use std::fs;

#[allow(dead_code)]
pub const FLAGS_WE_CARE_ABOUT: [&str; 5] = [
    "RELEASE_PLATFORM_SDK_VERSION",
    "RELEASE_PLATFORM_SDK_VERSION_FULL",
    "RELEASE_PLATFORM_VERSION",
    "RELEASE_PLATFORM_VERSION_CODENAME",
    "RELEASE_HIDDEN_API_EXPORTABLE_STUBS",
];

// A map of release-config -name -> map of flag-name -> flag-value
//
// Example access:
//
//   assert_eq!(BUILD_FLAGS["trunk"]["RELEASE_PLATFORM_SDK_VERSION"], 36)
#[allow(dead_code)]
pub type BuildFlagMap = HashMap<String, HashMap<String, String>>;

#[allow(dead_code)]
pub type AliasMap = HashMap<String, String>;

#[allow(dead_code)]
pub struct ReleaseConfigs {
    pub flags: BuildFlagMap,
    pub aliases: AliasMap,
}

impl ReleaseConfigs {
    #[allow(dead_code)]
    pub fn init() -> Self {
        let protobuf =
            fs::read("all_release_configs.pb").expect("Could not read all_release_configs.pb");
        let all_release_configs = ReleaseConfigsArtifact::parse_from_bytes(&protobuf[..])
            .expect("failed to parse protobuf as ReleaseConfigArtifact");

        let mut flags = HashMap::new();
        let mut aliases = HashMap::new();

        // parse currently active release config
        parse_release_config(&all_release_configs.release_config, &mut flags, &mut aliases);

        // parse the other release configs
        for release_config in all_release_configs.other_release_configs {
            parse_release_config(&release_config, &mut flags, &mut aliases);
        }

        ReleaseConfigs { flags, aliases }
    }
}

fn parse_release_config(
    release_config: &ReleaseConfigArtifact,
    build_flag_map: &mut BuildFlagMap,
    aliases: &mut AliasMap,
) {
    let flags: HashMap<String, String> = release_config
        .flags
        .iter()
        .filter(|flag| FLAGS_WE_CARE_ABOUT.contains(&flag.flag_declaration.name()))
        .map(|flag| {
            // Flag values are expected to be strings or bools, or not set. In this tool, we
            // represent all types as strings (for simplicity).
            let value = if flag.value.val.is_none() {
                // value not set -> ""
                String::new()
            } else if flag.value.has_string_value() {
                // already a string, use as is
                flag.value.string_value().to_string()
            } else if flag.value.has_bool_value() {
                // convert bool to "true" or "false"
                format!("{}", flag.value.bool_value())
            } else {
                panic!("unexpected protobuf value type: {:?}", flag.value);
            };
            (flag.flag_declaration.name().to_string(), value)
        })
        .collect();
    build_flag_map.insert(release_config.name().to_string(), flags);
    for alias in release_config.other_names.clone() {
        aliases.insert(alias, release_config.name().to_string());
    }
}
