/*
 * Copyright (C) 2024 The Android Open Source Project
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

//! `aflags` is a device binary to read and write aconfig flags.
// This binary has been replaced by the updatable `aflags` binary.

use std::env;
use std::process::{Command as OsCommand, Stdio};

use anyhow::Result;

fn invoke_updatable_aflags() {
    let updatable_command = "/apex/com.android.configinfrastructure/bin/aflags_updatable";

    let args: Vec<String> = env::args().collect();
    let default_command_args = ["--help".to_string()];
    let command_args = if args.len() >= 2 { &args[1..] } else { &default_command_args };

    let mut child = OsCommand::new(updatable_command);
    for arg in command_args {
        child.arg(arg);
    }

    let output = child
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to execute child")
        .wait_with_output()
        .expect("failed to execute command");

    let output_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output_str.is_empty() {
        println!("{output_str}");
    }
}

fn main() -> Result<()> {
    invoke_updatable_aflags();
    Ok(())
}
