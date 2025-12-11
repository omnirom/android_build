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

package android.aconfig.storage;

/**
 * Implementation to include in the build when RELEASE_ACONFIG_PARSE_V4 is false.
 *
 * Parsing (reading/writing) aconfig storage files for a new version should be flag-guarded, so that
 * if somehow an old library was presented with new files, it would not try to run the new code.
 *
 * A read-only aconfig flag can't be used, because the reader library is also used in rust, and
 * this would introduce a circular dependency. Therefore, a build flag must be used, and this
 * helper class is used to share the flag value between the Java and native libraries.
 *
 * The correct class definition will be selected at build time via the Android.bp file.
 */
public final class ParseV4FlagWrapper {
    public static boolean enabled() {
        return false;
    }
}
