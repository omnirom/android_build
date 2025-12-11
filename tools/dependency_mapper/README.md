// Copyright (C) 2025 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
# Dependency Mapper

[dependency-mapper] command line tool. This tool finds the usage based dependencies between java
files by utilizing byte-code and java file analysis.

# Getting Started

## Inputs
* src-path: .rsp file, containing list of java files separated by whitespace.
* jar-path: .jar file, containing class files generated after compiling the contents of java sources.
* cross-module-jar-list: .rsp file, containing list of jar files compiled alongside java sources (i.e. from kotlin sources) 

## Output
* dependency-map-path: .proto file, representing the list of dependencies for each java file present in input rsp file,
represented by [proto/dependency.proto]

## Usage
```
dependency-mapper --src-path <src-list.rsp> --jar-path <classes.jar>  --cross-module-jar-list <jar-list.rsp> --dependency-map-path <usage-map.proto>
```

# Notes
* Dependencies enlisted are only within the java files present in input.
* To ensure dependencies are listed correctly classes jar should contain every class files generated from each source file.
* Run `m dependency-mapper-test-data` when adding new testfiles, then copy the output jar to tests/res/testfiles location.