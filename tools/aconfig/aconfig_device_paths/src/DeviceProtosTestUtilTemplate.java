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
package android.aconfig;

import android.aconfig.nano.Aconfig.parsed_flag;
import android.aconfig.nano.Aconfig.parsed_flags;

import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/**
 * Utility class to load protobuf storage files.
 *
 * This class _does_ support Ravenwood.
 *
 * In order to avoid adding extra dependencies, this class doesn't use Ravenwood annotations
 * or RavenwoodHelper.java. Instead, we just hardcode relevant logic.
 *
 * @hide
 */
public class DeviceProtosTestUtil {
    private static final String[] PATHS_DEVICE = {
        TEMPLATE
    };

    /** Path to ravenwood runtime, or null on non-ravenwood environment. */
    private static final String RAVENWOOD_RUNTIME_PATH
            = System.getProperty("android.ravenwood.runtime_path");

    /** True if on ravenwood */
    private static final boolean ON_RAVENWOOD = RAVENWOOD_RUNTIME_PATH != null;

    private static String[] getPaths() {
        if (!ON_RAVENWOOD) {
            return PATHS_DEVICE;
        }
        return new String[] {
            RAVENWOOD_RUNTIME_PATH + "/aconfig/metadata/aconfig/etc/all_aconfig_declarations.pb"
        };
    }

    /**
     * Protobuf storage files. On the device side, this array contains multiple files, one
     * from each partition. On Ravenwood, this contains a single protobuf file containing all the
     * flags.
     */
    public static final String[] PATHS = getPaths();

    private static final String APEX_DIR = "/apex/";
    private static final String APEX_ACONFIG_PATH_SUFFIX = "/etc/aconfig_flags.pb";
    private static final String SYSTEM_APEX_DIR = "/system/apex";

    /**
     * Returns a list of all on-device aconfig protos.
     *
     * <p>May throw an exception if the protos can't be read at the call site. For example, some of
     * the protos are in the apex/ partition, which is mounted somewhat late in the boot process.
     *
     * @throws IOException if we can't read one of the protos yet
     * @return a list of all on-device aconfig protos
     */
    public static List<parsed_flag> loadAndParseFlagProtos() throws IOException {
        ArrayList<parsed_flag> result = new ArrayList();

        for (String path : parsedFlagsProtoPaths()) {
            try (FileInputStream inputStream = new FileInputStream(path)) {
                parsed_flags parsedFlags = parsed_flags.parseFrom(inputStream.readAllBytes());
                for (parsed_flag flag : parsedFlags.parsedFlag) {
                    result.add(flag);
                }
            }
        }

        return result;
    }

    /**
     * Returns the list of all on-device aconfig protos paths.
     *
     * @hide
     */
    public static List<String> parsedFlagsProtoPaths() {
        ArrayList<String> paths = new ArrayList(Arrays.asList(PATHS));

        if (ON_RAVENWOOD) {
            return paths; // No apexes on Ravenwood.
        }

        File apexDirectory = new File(SYSTEM_APEX_DIR);
        if (!apexDirectory.isDirectory()) {
            return paths;
        }

        File[] subdirs = apexDirectory.listFiles();
        if (subdirs == null) {
            return paths;
        }

        for (File prefix : subdirs) {
            String apexName = prefix.getName().replace("com.google", "com");
            apexName = apexName.substring(0, apexName.lastIndexOf('.'));

            File protoPath = new File(APEX_DIR + apexName + APEX_ACONFIG_PATH_SUFFIX);
            if (!protoPath.exists()) {
                continue;
            }

            paths.add(protoPath.getAbsolutePath());
        }
        return paths;
    }
}
