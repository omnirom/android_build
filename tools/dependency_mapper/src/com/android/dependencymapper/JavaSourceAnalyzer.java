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
package com.android.dependencymapper;

import java.io.BufferedReader;
import java.io.FileReader;
import java.io.IOException;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * An utility class that reads each java file present in the rsp content then analyzes the same,
 * collecting the analysis in {@link List<JavaSourceData>}
 */
public class JavaSourceAnalyzer {

    // Regex that matches against "package abc.xyz.lmn;" declarations in a java file.
    private static final String PACKAGE_REGEX = "^\\s*package\\s+([a-zA-Z_][a-zA-Z0-9_.]*);";

    public static List<JavaSourceData> parse(Path srcRspFile) {
        Set<String> files = Utils.parseRspFile(srcRspFile);
        List<JavaSourceData> javaSourceDataList = new ArrayList<>();
        for (String file : files) {
            javaSourceDataList.add(
                    new JavaSourceData(file, constructPackagePrependedFileName(file)));
        }
        return javaSourceDataList;
    }

    private static String constructPackagePrependedFileName(String filePath) {
        String packageAppendedFileName = null;
        // if the file path is abc/def/ghi/JavaFile.java we extract JavaFile.java
        String packagePrependedFileName = filePath.substring(filePath.lastIndexOf("/") + 1);
        try (BufferedReader reader = new BufferedReader(new FileReader(filePath))) {
            String line;
            // Process each line and match against the package regex pattern.
            while ((line = reader.readLine()) != null) {
                Pattern pattern = Pattern.compile(PACKAGE_REGEX);
                Matcher matcher = pattern.matcher(line);
                if (matcher.find()) {
                    packagePrependedFileName = matcher.group(1) + "." + packagePrependedFileName;
                    break;
                }
            }
        } catch (IOException e) {
            System.err.println("Error reading java file at: " + filePath);
            throw new RuntimeException(e);
        }
        return packagePrependedFileName;
    }
}
