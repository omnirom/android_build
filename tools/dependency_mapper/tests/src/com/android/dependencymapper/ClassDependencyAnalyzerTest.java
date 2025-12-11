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

import static com.android.dependencymapper.Utils.listClassesInJar;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

import org.junit.BeforeClass;
import org.junit.Test;

import java.net.URISyntaxException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.function.Function;

public class ClassDependencyAnalyzerTest {

    private static List<ClassDependencyData> mClassDependencyDataList;
    private static List<ClassDependencyData> mCrossModuleClassDependencyDataList;

    private static final String CLASSES_JAR_PATH =
            "tests/res/testfiles/dependency-mapper-test-data.jar";

    @BeforeClass
    public static void beforeClass() throws URISyntaxException {
        Path path = Paths.get(CLASSES_JAR_PATH);
        Set<String> classesInJar = listClassesInJar(path);
        // Perform dependency analysis.
        mClassDependencyDataList = ClassDependencyAnalyzer.analyze(
                path,
                new ClassRelevancyFilter(classesInJar),
                new ClassRelevancyFilter(new HashSet<>()));

        // mCrossModuleClassDependencyDataList functions and is tested identically to
        // mClassDependencyDataList except that it filters the dependencies into a
        // different list within
        mCrossModuleClassDependencyDataList = ClassDependencyAnalyzer.analyze(
                path,
                new ClassRelevancyFilter(new HashSet<>()),
                new ClassRelevancyFilter(classesInJar));
    }

    @Test
    public void testAnnotationDeps() {
        String annoClass = "res.testdata.annotation.AnnotationUsage";
        String sourceAnno = "res.testdata.annotation.SourceAnnotation";
        String runTimeAnno = "res.testdata.annotation.RuntimeAnnotation";

        dependencyVerifier(
                annoClass,
                new HashSet<>(List.of(runTimeAnno)),
                new HashSet<>(List.of(sourceAnno)),
                mClassDependencyDataList,
                ClassDependencyData::getClassDependencies);

        dependencyVerifier(
                annoClass,
                new HashSet<>(List.of(runTimeAnno)),
                new HashSet<>(List.of(sourceAnno)),
                mCrossModuleClassDependencyDataList,
                ClassDependencyData::getCrossModuleClassDependencies);

        for (ClassDependencyData dep : mClassDependencyDataList) {
            if (dep.getQualifiedName().equals(sourceAnno)) {
                assertTrue(sourceAnno + " is not dependencyToAll ", dep.isDependencyToAll());
            }
            if (dep.getQualifiedName().equals(runTimeAnno)) {
                assertFalse(runTimeAnno + " is dependencyToAll ", dep.isDependencyToAll());
            }
        }
    }

    @Test
    public void testConstantsDeps() {
        String constDefined = "test_constant";
        String constDefClass = "res.testdata.constants.ConstantDefinition";
        String constUsageClass = "res.testdata.constants.ConstantUsage";

        boolean constUsageClassFound = false;
        boolean constDefClassFound = false;
        for (ClassDependencyData dep : mClassDependencyDataList) {
            if (dep.getQualifiedName().equals(constUsageClass)) {
                constUsageClassFound = true;
                assertTrue("InlinedUsage of : " + constDefined + " not found",
                        dep.inlinedUsages().contains(constDefined));
            }
            if (dep.getQualifiedName().equals(constDefClass)) {
                constDefClassFound = true;
                assertTrue("Constant " + constDefined + " not defined",
                        dep.getConstantsDefined().contains(constDefined));
            }
        }
        assertTrue("Class " + constUsageClass + " not found", constUsageClassFound);
        assertTrue("Class " + constDefClass + " not found", constDefClassFound);
    }

    @Test
    public void testInheritanceDeps() {
        String sourceClass = "res.testdata.inheritance.InheritanceUsage";
        String baseClass = "res.testdata.inheritance.BaseClass";
        String baseImpl = "res.testdata.inheritance.BaseImpl";

        dependencyVerifier(
                sourceClass,
                new HashSet<>(List.of(baseClass, baseImpl)),
                new HashSet<>(),
                mClassDependencyDataList,
                ClassDependencyData::getClassDependencies);


        dependencyVerifier(
                sourceClass,
                new HashSet<>(List.of(baseClass, baseImpl)),
                new HashSet<>(),
                mCrossModuleClassDependencyDataList,
                ClassDependencyData::getCrossModuleClassDependencies);
    }


    @Test
    public void testMethodDeps() {
        String fieldUsage1 = "res.testdata.methods.FieldUsage";
        String fieldUsage2 = "WithoutPackageClass";
        String methodUsage = "res.testdata.methods.MethodUsage";
        String ref1 = "res.testdata.methods.ReferenceClass1";
        String ref2 = "res.testdata.methods.ReferenceClass2";

        dependencyVerifier(
                fieldUsage1,
                new HashSet<>(List.of(ref1)),
                new HashSet<>(List.of(ref2)),
                mClassDependencyDataList,
                ClassDependencyData::getClassDependencies);
        dependencyVerifier(
                fieldUsage2,
                new HashSet<>(List.of(ref1)),
                new HashSet<>(List.of(ref2)),
                mClassDependencyDataList,
                ClassDependencyData::getClassDependencies);
        dependencyVerifier(
                methodUsage,
                new HashSet<>(List.of(ref1, ref2)),
                new HashSet<>(),
                mClassDependencyDataList,
                ClassDependencyData::getClassDependencies);

        dependencyVerifier(
                fieldUsage1,
                new HashSet<>(List.of(ref1)),
                new HashSet<>(List.of(ref2)),
                mCrossModuleClassDependencyDataList,
                ClassDependencyData::getCrossModuleClassDependencies);
        dependencyVerifier(
                fieldUsage2,
                new HashSet<>(List.of(ref1)),
                new HashSet<>(List.of(ref2)),
                mCrossModuleClassDependencyDataList,
                ClassDependencyData::getCrossModuleClassDependencies);
        dependencyVerifier(
                methodUsage,
                new HashSet<>(List.of(ref1, ref2)),
                new HashSet<>(),
                mCrossModuleClassDependencyDataList,
                ClassDependencyData::getCrossModuleClassDependencies);
    }

    private void dependencyVerifier(
            String qualifiedName,
            Set<String> deps,
            Set<String> nonDeps,
            List<ClassDependencyData> classDependencyDataList,
            Function<ClassDependencyData, Set<String>> dataAccessor) {
        boolean depFound = false;
        for (ClassDependencyData classDependencyData : classDependencyDataList) {
            if (classDependencyData.getQualifiedName().equals(qualifiedName)) {
                depFound = true;
                for (String dep : deps) {
                    assertTrue(qualifiedName + " does not depends on " + dep,
                            dataAccessor.apply(classDependencyData).contains(dep));
                }
                for (String nonDep : nonDeps) {
                    assertFalse(qualifiedName + " depends on " + nonDep,
                            dataAccessor.apply(classDependencyData).contains(nonDep));
                }
            }
            if (depFound) {
                // Early exit on success.
                return;
            }
        }
        fail("Class " + qualifiedName + " not found");
    }
}
