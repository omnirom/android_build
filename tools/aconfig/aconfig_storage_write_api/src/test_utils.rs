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

use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use tempfile::NamedTempFile;

/// Create temp file copy
pub(crate) fn copy_to_temp_file(relative_path: &str, read_only: bool) -> Result<NamedTempFile> {
    let source_file = get_test_data_path(relative_path);
    let file = NamedTempFile::new()?;
    fs::copy(source_file, file.path())?;
    if read_only {
        let file_name = file.path().display().to_string();
        let mut perms = fs::metadata(file_name).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(file.path(), perms.clone()).unwrap();
    }
    Ok(file)
}

fn get_test_data_path(relative_path: &str) -> PathBuf {
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        // Running with cargo, construct the path to the data files
        // relative to aconfig_storage_read_api's manifest.
        let mut path = PathBuf::from(manifest_dir);
        path.pop(); // .../aconfig
        path.push("aconfig_storage_file");
        path.push("tests");
        path.push(relative_path);
        path
    } else {
        // Running with atest, test data is in the current directory
        PathBuf::from(relative_path)
    }
}
