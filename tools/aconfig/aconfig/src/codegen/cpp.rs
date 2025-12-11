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

use anyhow::{ensure, Result};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use tinytemplate::TinyTemplate;

use aconfig_protos::{
    ParsedFlagExt, ProtoFlagPermission, ProtoFlagState, ProtoFlagStorageBackend, ProtoParsedFlag,
};

use crate::codegen::{self, get_flag_offset_in_storage_file, CodegenMode};
use crate::commands::OutputFile;

pub fn generate_cpp_code<I>(
    package: &str,
    parsed_flags_iter: I,
    codegen_mode: CodegenMode,
    flag_ids: HashMap<String, u16>,
    package_fingerprint: Option<u64>,
) -> Result<Vec<OutputFile>>
where
    I: Iterator<Item = ProtoParsedFlag>,
{
    let mut readwrite_count = 0;
    let class_elements = parsed_flags_iter
        .map(|pf| create_class_element(package, &pf, flag_ids.clone(), &mut readwrite_count))
        .collect::<Result<Vec<ClassElement>>>()?;
    let readwrite = readwrite_count > 0;
    let has_fixed_read_only = class_elements.iter().any(|item| item.is_fixed_read_only);
    let header = package.replace('.', "_");
    let package_macro = header.to_uppercase();
    let cpp_namespace = package.replace('.', "::");
    ensure!(class_elements.len() > 0);
    let container = class_elements[0].container.clone();
    ensure!(codegen::is_valid_name_ident(&header));
    let use_package_fingerprint = package_fingerprint.is_some();
    let context = Context {
        header: &header,
        package_macro: &package_macro,
        cpp_namespace: &cpp_namespace,
        package,
        has_fixed_read_only,
        readwrite,
        readwrite_count,
        is_test_mode: codegen_mode == CodegenMode::Test,
        class_elements,
        container,
        use_package_fingerprint,
        package_fingerprint: package_fingerprint.unwrap_or_default(),
    };

    let files = [
        FileSpec {
            name: &format!("{header}.h"),
            template: include_str!("../../templates/cpp_exported_header.template"),
            dir: "include",
        },
        FileSpec {
            name: &format!("{header}.cc"),
            template: include_str!("../../templates/cpp_source_file.template"),
            dir: "",
        },
    ];
    files.iter().map(|file| generate_file(file, &context)).collect()
}

pub fn generate_file(file: &FileSpec, context: &Context) -> Result<OutputFile> {
    let mut template = TinyTemplate::new();
    template.add_template(file.name, file.template)?;
    let contents = template.render(file.name, &context)?;
    let path: PathBuf = [&file.dir, &file.name].iter().collect();
    Ok(OutputFile { contents: contents.into(), path })
}

#[derive(Serialize)]
pub struct FileSpec<'a> {
    pub name: &'a str,
    pub template: &'a str,
    pub dir: &'a str,
}

#[derive(Serialize)]
pub struct Context<'a> {
    pub header: &'a str,
    pub package_macro: &'a str,
    pub cpp_namespace: &'a str,
    pub package: &'a str,
    pub has_fixed_read_only: bool,
    pub readwrite: bool,
    pub readwrite_count: i32,
    pub is_test_mode: bool,
    pub class_elements: Vec<ClassElement>,
    pub container: String,
    pub use_package_fingerprint: bool,
    pub package_fingerprint: u64,
}

#[derive(Serialize)]
pub struct ClassElement {
    pub readwrite_idx: i32,
    pub readwrite: bool,
    pub is_fixed_read_only: bool,
    pub default_value: String,
    pub flag_name: String,
    pub flag_macro: String,
    pub flag_offset: u16,
    pub device_config_namespace: String,
    pub device_config_flag: String,
    pub container: String,
}

fn create_class_element(
    package: &str,
    pf: &ProtoParsedFlag,
    flag_ids: HashMap<String, u16>,
    rw_count: &mut i32,
) -> Result<ClassElement> {
    ensure!(
        pf.metadata.storage() != ProtoFlagStorageBackend::DEVICE_CONFIG,
        "device config storage backend cannot be used in native codegen for flag {}",
        pf.fully_qualified_name()
    );
    Ok(ClassElement {
        readwrite_idx: if pf.permission() == ProtoFlagPermission::READ_WRITE {
            let index = *rw_count;
            *rw_count += 1;
            index
        } else {
            -1
        },
        readwrite: pf.permission() == ProtoFlagPermission::READ_WRITE,
        is_fixed_read_only: pf.is_fixed_read_only(),
        default_value: if pf.state() == ProtoFlagState::ENABLED {
            "true".to_string()
        } else {
            "false".to_string()
        },
        flag_name: pf.name().to_string(),
        flag_macro: pf.name().to_uppercase(),
        flag_offset: get_flag_offset_in_storage_file(&flag_ids, pf)?,
        device_config_namespace: pf.namespace().to_string(),
        device_config_flag: codegen::create_device_config_ident(package, pf.name())
            .expect("values checked at flag parse time"),
        container: pf.container().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aconfig_protos::ProtoParsedFlags;
    use std::collections::HashMap;

    const EXPORTED_PROD_HEADER_EXPECTED: &str = r#"
#pragma once

// Avoid destruction for thread safety.
// Only enable this with clang.
#if defined(__clang__)
#ifndef ACONFIG_NO_DESTROY
#define ACONFIG_NO_DESTROY [[clang::no_destroy]]
#endif
#else
#warning "not built with clang disable no_destroy"
#ifndef ACONFIG_NO_DESTROY
#define ACONFIG_NO_DESTROY
#endif
#endif

#ifndef COM_ANDROID_ACONFIG_TEST
#define COM_ANDROID_ACONFIG_TEST(FLAG) COM_ANDROID_ACONFIG_TEST_##FLAG
#endif

#ifndef COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO
#define COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO true
#endif

#ifndef COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO_EXPORTED
#define COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO_EXPORTED true
#endif

#ifdef __cplusplus

#include <memory>

namespace com::android::aconfig::test {

class flag_provider_interface {
public:
    virtual ~flag_provider_interface() = default;

    virtual bool disabled_ro() = 0;

    virtual bool disabled_rw() = 0;

    virtual bool disabled_rw_exported() = 0;

    virtual bool disabled_rw_in_other_namespace() = 0;

    virtual bool enabled_fixed_ro() = 0;

    virtual bool enabled_fixed_ro_exported() = 0;

    virtual bool enabled_ro() = 0;

    virtual bool enabled_ro_exported() = 0;

    virtual bool enabled_rw() = 0;
};

ACONFIG_NO_DESTROY extern std::unique_ptr<flag_provider_interface> provider_;

inline bool disabled_ro() {
    return false;
}

inline bool disabled_rw() {
    return provider_->disabled_rw();
}

inline bool disabled_rw_exported() {
    return provider_->disabled_rw_exported();
}

inline bool disabled_rw_in_other_namespace() {
    return provider_->disabled_rw_in_other_namespace();
}

constexpr inline bool enabled_fixed_ro() {
    return COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO;
}

constexpr inline bool enabled_fixed_ro_exported() {
    return COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO_EXPORTED;
}

inline bool enabled_ro() {
    return true;
}

inline bool enabled_ro_exported() {
    return true;
}

inline bool enabled_rw() {
    return provider_->enabled_rw();
}

}

extern "C" {
#endif // __cplusplus

bool com_android_aconfig_test_disabled_ro();

bool com_android_aconfig_test_disabled_rw();

bool com_android_aconfig_test_disabled_rw_exported();

bool com_android_aconfig_test_disabled_rw_in_other_namespace();

bool com_android_aconfig_test_enabled_fixed_ro();

bool com_android_aconfig_test_enabled_fixed_ro_exported();

bool com_android_aconfig_test_enabled_ro();

bool com_android_aconfig_test_enabled_ro_exported();

bool com_android_aconfig_test_enabled_rw();

#ifdef __cplusplus
} // extern "C"
#endif
"#;

    const EXPORTED_TEST_HEADER_EXPECTED: &str = r#"
#pragma once

#ifdef __cplusplus

#include <memory>

namespace com::android::aconfig::test {

class flag_provider_interface {
public:

    virtual ~flag_provider_interface() = default;

    virtual bool disabled_ro() = 0;
    virtual bool disabled_rw() = 0;
    virtual bool disabled_rw_exported() = 0;
    virtual bool disabled_rw_in_other_namespace() = 0;
    virtual bool enabled_fixed_ro() = 0;
    virtual bool enabled_fixed_ro_exported() = 0;
    virtual bool enabled_ro() = 0;
    virtual bool enabled_ro_exported() = 0;
    virtual bool enabled_rw() = 0;

    virtual void disabled_ro(bool val) = 0;
    virtual void disabled_rw(bool val) = 0;
    virtual void disabled_rw_exported(bool val) = 0;
    virtual void disabled_rw_in_other_namespace(bool val) = 0;
    virtual void enabled_fixed_ro(bool val) = 0;
    virtual void enabled_fixed_ro_exported(bool val) = 0;
    virtual void enabled_ro(bool val) = 0;
    virtual void enabled_ro_exported(bool val) = 0;
    virtual void enabled_rw(bool val) = 0;

    virtual void reset_flags() {}
};

extern std::unique_ptr<flag_provider_interface> provider_;

inline bool disabled_ro() {
    return provider_->disabled_ro();
}

inline void disabled_ro(bool val) {
    provider_->disabled_ro(val);
}

inline bool disabled_rw() {
    return provider_->disabled_rw();
}

inline void disabled_rw(bool val) {
    provider_->disabled_rw(val);
}

inline bool disabled_rw_exported() {
    return provider_->disabled_rw_exported();
}

inline void disabled_rw_exported(bool val) {
    provider_->disabled_rw_exported(val);
}

inline bool disabled_rw_in_other_namespace() {
    return provider_->disabled_rw_in_other_namespace();
}

inline void disabled_rw_in_other_namespace(bool val) {
    provider_->disabled_rw_in_other_namespace(val);
}

inline bool enabled_fixed_ro() {
    return provider_->enabled_fixed_ro();
}

inline void enabled_fixed_ro(bool val) {
    provider_->enabled_fixed_ro(val);
}

inline bool enabled_fixed_ro_exported() {
    return provider_->enabled_fixed_ro_exported();
}

inline void enabled_fixed_ro_exported(bool val) {
    provider_->enabled_fixed_ro_exported(val);
}

inline bool enabled_ro() {
    return provider_->enabled_ro();
}

inline void enabled_ro(bool val) {
    provider_->enabled_ro(val);
}

inline bool enabled_ro_exported() {
    return provider_->enabled_ro_exported();
}

inline void enabled_ro_exported(bool val) {
    provider_->enabled_ro_exported(val);
}

inline bool enabled_rw() {
    return provider_->enabled_rw();
}

inline void enabled_rw(bool val) {
    provider_->enabled_rw(val);
}

inline void reset_flags() {
    return provider_->reset_flags();
}

}

extern "C" {
#endif // __cplusplus

bool com_android_aconfig_test_disabled_ro();

void set_com_android_aconfig_test_disabled_ro(bool val);

bool com_android_aconfig_test_disabled_rw();

void set_com_android_aconfig_test_disabled_rw(bool val);

bool com_android_aconfig_test_disabled_rw_exported();

void set_com_android_aconfig_test_disabled_rw_exported(bool val);

bool com_android_aconfig_test_disabled_rw_in_other_namespace();

void set_com_android_aconfig_test_disabled_rw_in_other_namespace(bool val);

bool com_android_aconfig_test_enabled_fixed_ro();

void set_com_android_aconfig_test_enabled_fixed_ro(bool val);

bool com_android_aconfig_test_enabled_fixed_ro_exported();

void set_com_android_aconfig_test_enabled_fixed_ro_exported(bool val);

bool com_android_aconfig_test_enabled_ro();

void set_com_android_aconfig_test_enabled_ro(bool val);

bool com_android_aconfig_test_enabled_ro_exported();

void set_com_android_aconfig_test_enabled_ro_exported(bool val);

bool com_android_aconfig_test_enabled_rw();

void set_com_android_aconfig_test_enabled_rw(bool val);

void com_android_aconfig_test_reset_flags();


#ifdef __cplusplus
} // extern "C"
#endif


"#;

    const EXPORTED_FORCE_READ_ONLY_HEADER_EXPECTED: &str = r#"
#pragma once

#ifndef COM_ANDROID_ACONFIG_TEST
#define COM_ANDROID_ACONFIG_TEST(FLAG) COM_ANDROID_ACONFIG_TEST_##FLAG
#endif

#ifndef COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO
#define COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO true
#endif

#ifdef __cplusplus

#include <memory>

namespace com::android::aconfig::test {

class flag_provider_interface {
public:
    virtual ~flag_provider_interface() = default;

    virtual bool disabled_ro() = 0;

    virtual bool disabled_rw() = 0;

    virtual bool disabled_rw_in_other_namespace() = 0;

    virtual bool enabled_fixed_ro() = 0;

    virtual bool enabled_ro() = 0;

    virtual bool enabled_rw() = 0;
};

extern std::unique_ptr<flag_provider_interface> provider_;

inline bool disabled_ro() {
    return false;
}

inline bool disabled_rw() {
    return false;
}

inline bool disabled_rw_in_other_namespace() {
    return false;
}

constexpr inline bool enabled_fixed_ro() {
    return COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO;
}

inline bool enabled_ro() {
    return true;
}

inline bool enabled_rw() {
    return true;
}

}

extern "C" {
#endif // __cplusplus

bool com_android_aconfig_test_disabled_ro();

bool com_android_aconfig_test_disabled_rw();

bool com_android_aconfig_test_disabled_rw_in_other_namespace();

bool com_android_aconfig_test_enabled_fixed_ro();

bool com_android_aconfig_test_enabled_ro();

bool com_android_aconfig_test_enabled_rw();

#ifdef __cplusplus
} // extern "C"
#endif
"#;

    const PROD_SOURCE_FILE_EXPECTED: &str = r#"
#include "com_android_aconfig_test.h"

#include <unistd.h>
#include "aconfig_storage/aconfig_storage_read_api.hpp"
#include <android/log.h>
#define LOG_TAG "aconfig_cpp_codegen"
#define ALOGE(...) __android_log_print(ANDROID_LOG_ERROR, LOG_TAG, __VA_ARGS__)
#include <atomic>
#include <vector>

namespace com::android::aconfig::test {

    class flag_provider : public flag_provider_interface {
        public:

            flag_provider()
                : cache_(4)
                , boolean_start_index_()
                , flag_value_file_(nullptr)
                , package_exists_in_storage_(true) {
                for (size_t i = 0 ; i < 4; i++) {
                    cache_[i] = -1;
                }

// Storage files are only available on Android, not on host.
#ifndef __ANDROID__
                package_exists_in_storage_ = false;
                return;
#endif

                auto package_map_file_ret = aconfig_storage::get_mapped_file(
                    "system",
                    aconfig_storage::StorageFileType::package_map);
                if (!package_map_file_ret.ok()) {
                    ALOGE("error: failed to get package map file: %s", package_map_file_ret.error().c_str());
                    package_exists_in_storage_ = false;
                    return;
                }
                std::unique_ptr<aconfig_storage::MappedStorageFile> package_map_file(*package_map_file_ret);
                auto context = aconfig_storage::get_package_read_context(
                    *package_map_file, "com.android.aconfig.test");
                if (!context.ok()) {
                    ALOGE("error: failed to get package read context: %s", context.error().c_str());
                    package_exists_in_storage_ = false;
                    return;
                }

                if (!(context->package_exists)) {
                    package_exists_in_storage_ = false;
                    return;
                }

                // cache package boolean flag start index
                boolean_start_index_ = context->boolean_start_index;

                auto flag_value_file = aconfig_storage::get_mapped_file(
                    "system",
                    aconfig_storage::StorageFileType::flag_val);
                if (!flag_value_file.ok()) {
                    ALOGE("error: failed to get flag value file: %s", flag_value_file.error().c_str());
                    package_exists_in_storage_ = false;
                    return;
                }

                // cache flag value file
                flag_value_file_ = std::unique_ptr<aconfig_storage::MappedStorageFile>(
                    *flag_value_file);

            }


            virtual bool disabled_ro() override {
                return false;
            }

            virtual bool disabled_rw() override {
                if (cache_[0].load(std::memory_order_relaxed) == -1) {
                    if (!package_exists_in_storage_) {
                        return false;
                    }

                    auto value = aconfig_storage::get_boolean_flag_value(
                        *flag_value_file_,
                        boolean_start_index_ + 0);

                    if (!value.ok()) {
                        ALOGE("error: failed to read flag value: %s", value.error().c_str());
                        return false;
                    }

                    cache_[0].store(*value, std::memory_order_relaxed);
                }
                return cache_[0].load(std::memory_order_relaxed);
            }

            virtual bool disabled_rw_exported() override {
                if (cache_[1].load(std::memory_order_relaxed) == -1) {
                    if (!package_exists_in_storage_) {
                        return false;
                    }

                    auto value = aconfig_storage::get_boolean_flag_value(
                        *flag_value_file_,
                        boolean_start_index_ + 1);

                    if (!value.ok()) {
                        ALOGE("error: failed to read flag value: %s", value.error().c_str());
                        return false;
                    }

                    cache_[1].store(*value, std::memory_order_relaxed);
                }
                return cache_[1].load(std::memory_order_relaxed);
            }

            virtual bool disabled_rw_in_other_namespace() override {
                if (cache_[2].load(std::memory_order_relaxed) == -1) {
                    if (!package_exists_in_storage_) {
                        return false;
                    }

                    auto value = aconfig_storage::get_boolean_flag_value(
                        *flag_value_file_,
                        boolean_start_index_ + 2);

                    if (!value.ok()) {
                        ALOGE("error: failed to read flag value: %s", value.error().c_str());
                        return false;
                    }

                    cache_[2].store(*value, std::memory_order_relaxed);
                }
                return cache_[2].load(std::memory_order_relaxed);
            }

            virtual bool enabled_fixed_ro() override {
                return COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO;
            }

            virtual bool enabled_fixed_ro_exported() override {
                return COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO_EXPORTED;
            }

            virtual bool enabled_ro() override {
                return true;
            }

            virtual bool enabled_ro_exported() override {
                return true;
            }

            virtual bool enabled_rw() override {
                if (cache_[3].load(std::memory_order_relaxed) == -1) {
                    if (!package_exists_in_storage_) {
                        return true;
                    }

                    auto value = aconfig_storage::get_boolean_flag_value(
                        *flag_value_file_,
                        boolean_start_index_ + 7);

                    if (!value.ok()) {
                        ALOGE("error: failed to read flag value: %s", value.error().c_str());
                        return true;
                    }

                    cache_[3].store(*value, std::memory_order_relaxed);
                }
                return cache_[3].load(std::memory_order_relaxed);
            }

    private:
        std::vector<std::atomic_int8_t> cache_;

        uint32_t boolean_start_index_;

        std::unique_ptr<aconfig_storage::MappedStorageFile> flag_value_file_;

        bool package_exists_in_storage_;

    };

    static flag_provider_interface* get_provider_instance() {
        static flag_provider* instance_ = new flag_provider();
        return instance_;
    }

    std::unique_ptr<flag_provider_interface> provider_(get_provider_instance());
}

bool com_android_aconfig_test_disabled_ro() {
    return false;
}

bool com_android_aconfig_test_disabled_rw() {
    return com::android::aconfig::test::disabled_rw();
}

bool com_android_aconfig_test_disabled_rw_exported() {
    return com::android::aconfig::test::disabled_rw_exported();
}

bool com_android_aconfig_test_disabled_rw_in_other_namespace() {
    return com::android::aconfig::test::disabled_rw_in_other_namespace();
}

bool com_android_aconfig_test_enabled_fixed_ro() {
    return COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO;
}

bool com_android_aconfig_test_enabled_fixed_ro_exported() {
    return COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO_EXPORTED;
}

bool com_android_aconfig_test_enabled_ro() {
    return true;
}

bool com_android_aconfig_test_enabled_ro_exported() {
    return true;
}

bool com_android_aconfig_test_enabled_rw() {
    return com::android::aconfig::test::enabled_rw();
}

"#;

    const PROD_SOURCE_FILE_EXPECTED_WITH_FINGERPRINT: &str = r#"
#include "com_android_aconfig_test.h"

#include <unistd.h>
#include "aconfig_storage/aconfig_storage_read_api.hpp"
#include <android/log.h>
#define LOG_TAG "aconfig_cpp_codegen"
#define ALOGE(...) __android_log_print(ANDROID_LOG_ERROR, LOG_TAG, __VA_ARGS__)
#include <atomic>
#include <vector>

namespace com::android::aconfig::test {

    class flag_provider : public flag_provider_interface {
        public:

            flag_provider()
                : cache_(4)
                , boolean_start_index_()
                , flag_value_file_(nullptr)
                , package_exists_in_storage_(true)
                , fingerprint_matches_(true) {
                for (size_t i = 0 ; i < 4; i++) {
                    cache_[i] = -1;
                }

// Storage files are only available on Android, not on host.
#ifndef __ANDROID__
                package_exists_in_storage_ = false;
                return;
#endif

                auto package_map_file_ret = aconfig_storage::get_mapped_file(
                    "system",
                    aconfig_storage::StorageFileType::package_map);
                if (!package_map_file_ret.ok()) {
                    ALOGE("error: failed to get package map file: %s", package_map_file_ret.error().c_str());
                    package_exists_in_storage_ = false;
                    return;
                }
                std::unique_ptr<aconfig_storage::MappedStorageFile> package_map_file(*package_map_file_ret);
                auto context = aconfig_storage::get_package_read_context(
                    *package_map_file, "com.android.aconfig.test");
                if (!context.ok()) {
                    ALOGE("error: failed to get package read context: %s", context.error().c_str());
                    package_exists_in_storage_ = false;
                    return;
                }

                if (!(context->package_exists)) {
                    package_exists_in_storage_ = false;
                    return;
                }

                if (context->fingerprint != 5801144784618221668ULL) {
                    ALOGE("Fingerprint mismatch for package com.android.aconfig.test.");
                    fingerprint_matches_ = false;
                    return;
                }

                // cache package boolean flag start index
                boolean_start_index_ = context->boolean_start_index;

                auto flag_value_file = aconfig_storage::get_mapped_file(
                    "system",
                    aconfig_storage::StorageFileType::flag_val);
                if (!flag_value_file.ok()) {
                    ALOGE("error: failed to get flag value file: %s", flag_value_file.error().c_str());
                    package_exists_in_storage_ = false;
                    return;
                }

                // cache flag value file
                flag_value_file_ = std::unique_ptr<aconfig_storage::MappedStorageFile>(
                    *flag_value_file);

            }


            virtual bool disabled_ro() override {
                return false;
            }

            virtual bool disabled_rw() override {
                if (cache_[0].load(std::memory_order_relaxed) == -1) {
                    if (!package_exists_in_storage_) {
                        return false;
                    }

                    if (!fingerprint_matches_) {
                        ALOGE("error: package fingerprint mismtach, returning flag default value.");
                        return false;
                    }

                    auto value = aconfig_storage::get_boolean_flag_value(
                        *flag_value_file_,
                        boolean_start_index_ + 0);

                    if (!value.ok()) {
                        ALOGE("error: failed to read flag value: %s", value.error().c_str());
                        return false;
                    }

                    cache_[0].store(*value, std::memory_order_relaxed);
                }
                return cache_[0].load(std::memory_order_relaxed);
            }

            virtual bool disabled_rw_exported() override {
                if (cache_[1].load(std::memory_order_relaxed) == -1) {
                    if (!package_exists_in_storage_) {
                        return false;
                    }


                    if (!fingerprint_matches_) {
                      ALOGE("error: package fingerprint mismtach, returning flag default value.");
                      return false;
                    }

                    auto value = aconfig_storage::get_boolean_flag_value(
                        *flag_value_file_,
                        boolean_start_index_ + 1);

                    if (!value.ok()) {
                        ALOGE("error: failed to read flag value: %s", value.error().c_str());
                        return false;
                    }

                    cache_[1].store(*value, std::memory_order_relaxed);
                }
                return cache_[1].load(std::memory_order_relaxed);
            }

            virtual bool disabled_rw_in_other_namespace() override {
                if (cache_[2].load(std::memory_order_relaxed) == -1) {
                    if (!package_exists_in_storage_) {
                        return false;
                    }


                    if (!fingerprint_matches_) {
                      ALOGE("error: package fingerprint mismtach, returning flag default value.");
                      return false;
                    }

                    auto value = aconfig_storage::get_boolean_flag_value(
                        *flag_value_file_,
                        boolean_start_index_ + 2);

                    if (!value.ok()) {
                        ALOGE("error: failed to read flag value: %s", value.error().c_str());
                        return false;
                    }

                    cache_[2].store(*value, std::memory_order_relaxed);
                }
                return cache_[2].load(std::memory_order_relaxed);
            }

            virtual bool enabled_fixed_ro() override {
                return COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO;
            }

            virtual bool enabled_fixed_ro_exported() override {
                return COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO_EXPORTED;
            }

            virtual bool enabled_ro() override {
                return true;
            }

            virtual bool enabled_ro_exported() override {
                return true;
            }

            virtual bool enabled_rw() override {
                if (cache_[3].load(std::memory_order_relaxed) == -1) {
                    if (!package_exists_in_storage_) {
                        return true;
                    }


                    if (!fingerprint_matches_) {
                      ALOGE("error: package fingerprint mismtach, returning flag default value.");
                      return true;
                    }

                    auto value = aconfig_storage::get_boolean_flag_value(
                        *flag_value_file_,
                        boolean_start_index_ + 7);

                    if (!value.ok()) {
                        ALOGE("error: failed to read flag value: %s", value.error().c_str());
                        return true;
                    }

                    cache_[3].store(*value, std::memory_order_relaxed);
                }
                return cache_[3].load(std::memory_order_relaxed);
            }

    private:
        std::vector<std::atomic_int8_t> cache_;

        uint32_t boolean_start_index_;

        std::unique_ptr<aconfig_storage::MappedStorageFile> flag_value_file_;

        bool package_exists_in_storage_;

        bool fingerprint_matches_;
    };

    static flag_provider_interface* get_provider_instance() {
        static flag_provider* instance_ = new flag_provider();
        return instance_;
    }

    std::unique_ptr<flag_provider_interface> provider_(get_provider_instance());
}

bool com_android_aconfig_test_disabled_ro() {
    return false;
}

bool com_android_aconfig_test_disabled_rw() {
    return com::android::aconfig::test::disabled_rw();
}

bool com_android_aconfig_test_disabled_rw_exported() {
    return com::android::aconfig::test::disabled_rw_exported();
}

bool com_android_aconfig_test_disabled_rw_in_other_namespace() {
    return com::android::aconfig::test::disabled_rw_in_other_namespace();
}

bool com_android_aconfig_test_enabled_fixed_ro() {
    return COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO;
}

bool com_android_aconfig_test_enabled_fixed_ro_exported() {
    return COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO_EXPORTED;
}

bool com_android_aconfig_test_enabled_ro() {
    return true;
}

bool com_android_aconfig_test_enabled_ro_exported() {
    return true;
}

bool com_android_aconfig_test_enabled_rw() {
    return com::android::aconfig::test::enabled_rw();
}

"#;

    const TEST_SOURCE_FILE_EXPECTED: &str = r#"
#include "com_android_aconfig_test.h"

#include <unistd.h>
#include "aconfig_storage/aconfig_storage_read_api.hpp"
#include <android/log.h>
#define LOG_TAG "aconfig_cpp_codegen"
#define ALOGE(...) __android_log_print(ANDROID_LOG_ERROR, LOG_TAG, __VA_ARGS__)

#include <unordered_map>
#include <string>

namespace com::android::aconfig::test {

    class flag_provider : public flag_provider_interface {
        private:
            std::unordered_map<std::string, bool> overrides_;

            uint32_t boolean_start_index_;

            std::unique_ptr<aconfig_storage::MappedStorageFile> flag_value_file_;

            bool package_exists_in_storage_;

        public:
            flag_provider()
                : overrides_()
                , boolean_start_index_()
                , flag_value_file_(nullptr)
                , package_exists_in_storage_(true) {

// Storage files are only available on Android, not on host.
#ifndef __ANDROID__
                package_exists_in_storage_ = false;
                return;
#endif

                auto package_map_file_ret = aconfig_storage::get_mapped_file(
                     "system",
                    aconfig_storage::StorageFileType::package_map);

                if (!package_map_file_ret.ok()) {
                    ALOGE("error: failed to get package map file: %s", package_map_file_ret.error().c_str());
                    package_exists_in_storage_ = false;
                    return;
                }
                std::unique_ptr<aconfig_storage::MappedStorageFile> package_map_file(*package_map_file_ret);
                auto context = aconfig_storage::get_package_read_context(
                    *package_map_file, "com.android.aconfig.test");

                if (!context.ok()) {
                    ALOGE("error: failed to get package read context: %s", context.error().c_str());
                    package_exists_in_storage_ = false;
                    return;
                }

                if (!(context->package_exists)) {
                    package_exists_in_storage_ = false;
                    return;
                }

                // cache package boolean flag start index
                boolean_start_index_ = context->boolean_start_index;

                auto flag_value_file = aconfig_storage::get_mapped_file(
                    "system",
                aconfig_storage::StorageFileType::flag_val);
                if (!flag_value_file.ok()) {
                    ALOGE("error: failed to get flag value file: %s", flag_value_file.error().c_str());
                    package_exists_in_storage_ = false;
                    return;
                }

                // cache flag value file
                flag_value_file_ = std::unique_ptr<aconfig_storage::MappedStorageFile>(
                *flag_value_file);

            }

            virtual bool disabled_ro() override {
                auto it = overrides_.find("disabled_ro");
                  if (it != overrides_.end()) {
                      return it->second;
                } else {
                  return false;
                }
            }

            virtual void disabled_ro(bool val) override {
                overrides_["disabled_ro"] = val;
            }

            virtual bool disabled_rw() override {
                auto it = overrides_.find("disabled_rw");
                  if (it != overrides_.end()) {
                      return it->second;
                } else {
                    if (!package_exists_in_storage_) {
                        return false;
                    }

                    auto value = aconfig_storage::get_boolean_flag_value(
                        *flag_value_file_,
                        boolean_start_index_ + 0);

                    if (!value.ok()) {
                        ALOGE("error: failed to read flag value: %s", value.error().c_str());
                        return false;
                    } else {
                        return *value;
                    }
                }
            }

            virtual void disabled_rw(bool val) override {
                overrides_["disabled_rw"] = val;
            }

            virtual bool disabled_rw_exported() override {
                auto it = overrides_.find("disabled_rw_exported");
                  if (it != overrides_.end()) {
                      return it->second;
                } else {
                    if (!package_exists_in_storage_) {
                        return false;
                    }

                    auto value = aconfig_storage::get_boolean_flag_value(
                        *flag_value_file_,
                        boolean_start_index_ + 1);

                    if (!value.ok()) {
                        ALOGE("error: failed to read flag value: %s", value.error().c_str());
                        return false;
                    } else {
                        return *value;
                    }
                }
            }

            virtual void disabled_rw_exported(bool val) override {
                overrides_["disabled_rw_exported"] = val;
            }

            virtual bool disabled_rw_in_other_namespace() override {
                auto it = overrides_.find("disabled_rw_in_other_namespace");
                  if (it != overrides_.end()) {
                      return it->second;
                } else {
                    if (!package_exists_in_storage_) {
                        return false;
                    }

                    auto value = aconfig_storage::get_boolean_flag_value(
                        *flag_value_file_,
                        boolean_start_index_ + 2);

                    if (!value.ok()) {
                        ALOGE("error: failed to read flag value: %s", value.error().c_str());
                        return false;
                    } else {
                        return *value;
                    }
                }
            }

            virtual void disabled_rw_in_other_namespace(bool val) override {
                overrides_["disabled_rw_in_other_namespace"] = val;
            }

            virtual bool enabled_fixed_ro() override {
                auto it = overrides_.find("enabled_fixed_ro");
                  if (it != overrides_.end()) {
                      return it->second;
                } else {
                  return true;
                }
            }

            virtual void enabled_fixed_ro(bool val) override {
                overrides_["enabled_fixed_ro"] = val;
            }

            virtual bool enabled_fixed_ro_exported() override {
                auto it = overrides_.find("enabled_fixed_ro_exported");
                  if (it != overrides_.end()) {
                      return it->second;
                } else {
                  return true;
                }
            }

            virtual void enabled_fixed_ro_exported(bool val) override {
                overrides_["enabled_fixed_ro_exported"] = val;
            }

            virtual bool enabled_ro() override {
                auto it = overrides_.find("enabled_ro");
                  if (it != overrides_.end()) {
                      return it->second;
                } else {
                  return true;
                }
            }

            virtual void enabled_ro(bool val) override {
                overrides_["enabled_ro"] = val;
            }

            virtual bool enabled_ro_exported() override {
                auto it = overrides_.find("enabled_ro_exported");
                  if (it != overrides_.end()) {
                      return it->second;
                } else {
                  return true;
                }
            }

            virtual void enabled_ro_exported(bool val) override {
                overrides_["enabled_ro_exported"] = val;
            }

            virtual bool enabled_rw() override {
                auto it = overrides_.find("enabled_rw");
                  if (it != overrides_.end()) {
                      return it->second;
                } else {
                    if (!package_exists_in_storage_) {
                        return true;
                    }

                    auto value = aconfig_storage::get_boolean_flag_value(
                        *flag_value_file_,
                        boolean_start_index_ + 7);

                    if (!value.ok()) {
                        ALOGE("error: failed to read flag value: %s", value.error().c_str());
                        return true;
                    } else {
                        return *value;
                    }
                }
            }

            virtual void enabled_rw(bool val) override {
                overrides_["enabled_rw"] = val;
            }

            virtual void reset_flags() override {
                overrides_.clear();
            }
    };

    static flag_provider_interface* get_provider_instance() {
        static flag_provider* instance_ = new flag_provider();
        return instance_;
    }

    std::unique_ptr<flag_provider_interface> provider_(get_provider_instance());
}

bool com_android_aconfig_test_disabled_ro() {
    return com::android::aconfig::test::disabled_ro();
}


void set_com_android_aconfig_test_disabled_ro(bool val) {
    com::android::aconfig::test::disabled_ro(val);
}

bool com_android_aconfig_test_disabled_rw() {
    return com::android::aconfig::test::disabled_rw();
}


void set_com_android_aconfig_test_disabled_rw(bool val) {
    com::android::aconfig::test::disabled_rw(val);
}


bool com_android_aconfig_test_disabled_rw_exported() {
    return com::android::aconfig::test::disabled_rw_exported();
}

void set_com_android_aconfig_test_disabled_rw_exported(bool val) {
    com::android::aconfig::test::disabled_rw_exported(val);
}


bool com_android_aconfig_test_disabled_rw_in_other_namespace() {
    return com::android::aconfig::test::disabled_rw_in_other_namespace();
}

void set_com_android_aconfig_test_disabled_rw_in_other_namespace(bool val) {
    com::android::aconfig::test::disabled_rw_in_other_namespace(val);
}


bool com_android_aconfig_test_enabled_fixed_ro() {
    return com::android::aconfig::test::enabled_fixed_ro();
}

void set_com_android_aconfig_test_enabled_fixed_ro(bool val) {
    com::android::aconfig::test::enabled_fixed_ro(val);
}

bool com_android_aconfig_test_enabled_fixed_ro_exported() {
    return com::android::aconfig::test::enabled_fixed_ro_exported();
}

void set_com_android_aconfig_test_enabled_fixed_ro_exported(bool val) {
    com::android::aconfig::test::enabled_fixed_ro_exported(val);
}

bool com_android_aconfig_test_enabled_ro() {
    return com::android::aconfig::test::enabled_ro();
}


void set_com_android_aconfig_test_enabled_ro(bool val) {
    com::android::aconfig::test::enabled_ro(val);
}


bool com_android_aconfig_test_enabled_ro_exported() {
    return com::android::aconfig::test::enabled_ro_exported();
}


void set_com_android_aconfig_test_enabled_ro_exported(bool val) {
    com::android::aconfig::test::enabled_ro_exported(val);
}


bool com_android_aconfig_test_enabled_rw() {
    return com::android::aconfig::test::enabled_rw();
}


void set_com_android_aconfig_test_enabled_rw(bool val) {
    com::android::aconfig::test::enabled_rw(val);
}

void com_android_aconfig_test_reset_flags() {
     com::android::aconfig::test::reset_flags();
}

"#;

    const FORCE_READ_ONLY_SOURCE_FILE_EXPECTED: &str = r#"
#include "com_android_aconfig_test.h"

namespace com::android::aconfig::test {

    class flag_provider : public flag_provider_interface {
        public:

            virtual bool disabled_ro() override {
                return false;
            }

            virtual bool disabled_rw() override {
                return false;
            }

            virtual bool disabled_rw_in_other_namespace() override {
                return false;
            }

            virtual bool enabled_fixed_ro() override {
                return COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO;
            }

            virtual bool enabled_ro() override {
                return true;
            }

            virtual bool enabled_rw() override {
                return true;
            }
    };

    static flag_provider_interface* get_provider_instance() {
        static flag_provider* instance_ = new flag_provider();
        return instance_;
    }

    std::unique_ptr<flag_provider_interface> provider_(get_provider_instance());
}

bool com_android_aconfig_test_disabled_ro() {
    return false;
}

bool com_android_aconfig_test_disabled_rw() {
    return false;
}

bool com_android_aconfig_test_disabled_rw_in_other_namespace() {
    return false;
}

bool com_android_aconfig_test_enabled_fixed_ro() {
    return COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO;
}

bool com_android_aconfig_test_enabled_ro() {
    return true;
}

bool com_android_aconfig_test_enabled_rw() {
    return true;
}

"#;

    const READ_ONLY_EXPORTED_PROD_HEADER_EXPECTED: &str = r#"
#pragma once

#ifndef COM_ANDROID_ACONFIG_TEST
#define COM_ANDROID_ACONFIG_TEST(FLAG) COM_ANDROID_ACONFIG_TEST_##FLAG
#endif

#ifndef COM_ANDROID_ACONFIG_TEST_DISABLED_FIXED_RO
#define COM_ANDROID_ACONFIG_TEST_DISABLED_FIXED_RO false
#endif

#ifndef COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO
#define COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO true
#endif

#ifdef __cplusplus

#include <memory>

namespace com::android::aconfig::test {

class flag_provider_interface {
public:
    virtual ~flag_provider_interface() = default;

    virtual bool disabled_fixed_ro() = 0;

    virtual bool disabled_ro() = 0;

    virtual bool enabled_fixed_ro() = 0;

    virtual bool enabled_ro() = 0;
};

extern std::unique_ptr<flag_provider_interface> provider_;

constexpr inline bool disabled_fixed_ro() {
    return COM_ANDROID_ACONFIG_TEST_DISABLED_FIXED_RO;
}

inline bool disabled_ro() {
    return false;
}

constexpr inline bool enabled_fixed_ro() {
    return COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO;
}

inline bool enabled_ro() {
    return true;
}
}

extern "C" {
#endif // __cplusplus

bool com_android_aconfig_test_disabled_fixed_ro();

bool com_android_aconfig_test_disabled_ro();

bool com_android_aconfig_test_enabled_fixed_ro();

bool com_android_aconfig_test_enabled_ro();

#ifdef __cplusplus
} // extern "C"
#endif
"#;

    const READ_ONLY_PROD_SOURCE_FILE_EXPECTED: &str = r#"
#include "com_android_aconfig_test.h"

namespace com::android::aconfig::test {

    class flag_provider : public flag_provider_interface {
        public:

            virtual bool disabled_fixed_ro() override {
                return COM_ANDROID_ACONFIG_TEST_DISABLED_FIXED_RO;
            }

            virtual bool disabled_ro() override {
                return false;
            }

            virtual bool enabled_fixed_ro() override {
                return COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO;
            }

            virtual bool enabled_ro() override {
                return true;
            }
    };

    static flag_provider_interface* get_provider_instance() {
        static flag_provider* instance_ = new flag_provider();
        return instance_;
    }

    std::unique_ptr<flag_provider_interface> provider_(get_provider_instance());
}

bool com_android_aconfig_test_disabled_fixed_ro() {
    return COM_ANDROID_ACONFIG_TEST_DISABLED_FIXED_RO;
}

bool com_android_aconfig_test_disabled_ro() {
    return false;
}

bool com_android_aconfig_test_enabled_fixed_ro() {
    return COM_ANDROID_ACONFIG_TEST_ENABLED_FIXED_RO;
}

bool com_android_aconfig_test_enabled_ro() {
    return true;
}
"#;
    use crate::commands::assign_flag_ids;

    fn test_generate_cpp_code(
        parsed_flags: ProtoParsedFlags,
        mode: CodegenMode,
        expected_header: &str,
        expected_src: &str,
        with_fingerprint: bool,
    ) {
        let modified_parsed_flags =
            crate::commands::modify_parsed_flags_based_on_mode(parsed_flags, mode).unwrap();
        let flag_ids =
            assign_flag_ids(crate::test::TEST_PACKAGE, modified_parsed_flags.iter()).unwrap();
        let package_fingerprint = if with_fingerprint { Some(5801144784618221668) } else { None };
        let generated = generate_cpp_code(
            crate::test::TEST_PACKAGE,
            modified_parsed_flags.into_iter(),
            mode,
            flag_ids,
            package_fingerprint,
        )
        .unwrap();
        let mut generated_files_map = HashMap::new();
        for file in generated {
            generated_files_map.insert(
                String::from(file.path.to_str().unwrap()),
                String::from_utf8(file.contents).unwrap(),
            );
        }

        let mut target_file_path = String::from("include/com_android_aconfig_test.h");
        assert!(generated_files_map.contains_key(&target_file_path));
        assert_eq!(
            None,
            crate::test::first_significant_code_diff(
                expected_header,
                generated_files_map.get(&target_file_path).unwrap()
            )
        );

        target_file_path = String::from("com_android_aconfig_test.cc");
        assert!(generated_files_map.contains_key(&target_file_path));
        crate::test::assert_no_significant_code_diff(
            expected_src,
            generated_files_map.get(&target_file_path).unwrap(),
        );
    }

    #[test]
    fn test_generate_cpp_code_for_prod() {
        let parsed_flags = crate::test::parse_test_flags();
        test_generate_cpp_code(
            parsed_flags,
            CodegenMode::Production,
            EXPORTED_PROD_HEADER_EXPECTED,
            PROD_SOURCE_FILE_EXPECTED,
            false,
        );
    }

    #[test]
    fn test_generate_cpp_code_for_prod_with_fingerprint() {
        let parsed_flags = crate::test::parse_test_flags();
        test_generate_cpp_code(
            parsed_flags,
            CodegenMode::Production,
            EXPORTED_PROD_HEADER_EXPECTED,
            PROD_SOURCE_FILE_EXPECTED_WITH_FINGERPRINT,
            true,
        );
    }

    #[test]
    fn test_generate_cpp_code_for_test() {
        let parsed_flags = crate::test::parse_test_flags();
        test_generate_cpp_code(
            parsed_flags,
            CodegenMode::Test,
            EXPORTED_TEST_HEADER_EXPECTED,
            TEST_SOURCE_FILE_EXPECTED,
            false,
        );
    }

    #[test]
    fn test_generate_cpp_code_for_force_read_only() {
        let parsed_flags = crate::test::parse_test_flags();
        test_generate_cpp_code(
            parsed_flags,
            CodegenMode::ForceReadOnly,
            EXPORTED_FORCE_READ_ONLY_HEADER_EXPECTED,
            FORCE_READ_ONLY_SOURCE_FILE_EXPECTED,
            false,
        );
    }

    #[test]
    fn test_generate_cpp_code_for_read_only_prod() {
        let parsed_flags = crate::test::parse_read_only_test_flags();
        test_generate_cpp_code(
            parsed_flags,
            CodegenMode::Production,
            READ_ONLY_EXPORTED_PROD_HEADER_EXPECTED,
            READ_ONLY_PROD_SOURCE_FILE_EXPECTED,
            false,
        );
    }
}
