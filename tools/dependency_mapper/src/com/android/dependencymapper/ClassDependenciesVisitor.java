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

import org.objectweb.asm.signature.SignatureReader;
import org.objectweb.asm.signature.SignatureVisitor;
import org.objectweb.asm.ClassReader;
import org.objectweb.asm.ClassVisitor;
import org.objectweb.asm.Label;
import org.objectweb.asm.Opcodes;
import org.objectweb.asm.Type;
import org.objectweb.asm.TypePath;

import java.lang.annotation.RetentionPolicy;
import java.util.HashSet;
import java.util.Set;

/**
 * An ASM based class visitor to analyze and club all dependencies of a java file.
 * Most of the logic of this class is inspired from
 * <a href="https://github.com/gradle/gradle/blob/master/platforms/jvm/language-java/src/main/java/org/gradle/api/internal/tasks/compile/incremental/asm/ClassDependenciesVisitor.java">gradle incremental compilation</a>
 */
public class ClassDependenciesVisitor extends ClassVisitor {

    private final static int API = Opcodes.ASM9;

    private final Set<String> mClassTypes = new HashSet<>();
    private final Set<String> mCrossModuleClassTypes = new HashSet<>();
    private final Set<Object> mConstantsDefined = new HashSet<>();
    private final Set<Object> mInlinedUsages = new HashSet<>();
    private String mSource;
    private boolean isAnnotationType;
    private boolean mIsDependencyToAll;
    private final RetentionPolicyVisitor mRetentionPolicyVisitor = new RetentionPolicyVisitor();

    private final ClassRelevancyFilter mClassFilter;
    private final ClassRelevancyFilter mCrossModuleFilter;

    private ClassDependenciesVisitor(
            ClassReader reader,
            ClassRelevancyFilter filter,
            ClassRelevancyFilter crossModuleFilter) {
        super(API);
        mClassFilter = filter;
        mCrossModuleFilter = crossModuleFilter;
        collectRemainingClassDependencies(reader);
    }

    public static ClassDependencyData analyze(
            String className,
            ClassReader reader,
            ClassRelevancyFilter filter,
            ClassRelevancyFilter crossModuleFilter) {
        ClassDependenciesVisitor visitor = new ClassDependenciesVisitor(
                reader, filter, crossModuleFilter);
        reader.accept(visitor, ClassReader.SKIP_FRAMES);
        // Sometimes a class may contain references to the same class, we remove such cases to
        // prevent circular dependency.
        visitor.getClassTypes().remove(className);
        return new ClassDependencyData(Utils.buildPackagePrependedClassSource(
                className, visitor.getSource()), className, visitor.getClassTypes(),
                visitor.isDependencyToAll(), visitor.getConstantsDefined(),
                visitor.getInlinedUsages(), visitor.getCrossModuleClassTypes());
    }


    @Override
    public void visitSource(String source, String debug) {
        mSource = source;
    }

    @Override
    public void visit(int version, int access, String name, String signature, String superName,
            String[] interfaces) {
        isAnnotationType = isAnnotationType(interfaces);
        maybeAddClassTypesFromSignature(signature, mClassTypes, mCrossModuleClassTypes);
        if (superName != null) {
            // superName can be null if what we are analyzing is `java.lang.Object`
            // which can happen when a custom Java SDK is on classpath (typically, android.jar)
            Type type = Type.getObjectType(superName);
            maybeAddClassType(mClassTypes, mCrossModuleClassTypes, type);
        }
        for (String s : interfaces) {
            Type interfaceType = Type.getObjectType(s);
            maybeAddClassType(mClassTypes, mCrossModuleClassTypes, interfaceType);
        }
    }

    // performs a fast analysis of classes referenced in bytecode (method bodies)
    // avoiding us to implement a costly visitor and potentially missing edge cases
    private void collectRemainingClassDependencies(ClassReader reader) {
        char[] charBuffer = new char[reader.getMaxStringLength()];
        for (int i = 1; i < reader.getItemCount(); i++) {
            int itemOffset = reader.getItem(i);
            // see https://docs.oracle.com/javase/specs/jvms/se7/html/jvms-4.html#jvms-4.4
            if (itemOffset > 0 && reader.readByte(itemOffset - 1) == 7) {
                // A CONSTANT_Class entry, read the class descriptor
                String classDescriptor = reader.readUTF8(itemOffset, charBuffer);
                Type type = Type.getObjectType(classDescriptor);
                maybeAddClassType(mClassTypes, mCrossModuleClassTypes, type);
            }
        }
    }

    private void maybeAddClassTypesFromSignature(String signature, Set<String> types,
            Set<String> crossModuleTypes) {
        if (signature != null) {
            SignatureReader signatureReader = new SignatureReader(signature);
            signatureReader.accept(new SignatureVisitor(API) {
                @Override
                public void visitClassType(String className) {
                    Type type = Type.getObjectType(className);
                    maybeAddClassType(types, crossModuleTypes, type);
                }
            });
        }
    }

    protected void maybeAddClassType(Set<String> types, Set<String> crossModuleTypes, Type type) {
        while (type.getSort() == Type.ARRAY) {
            type = type.getElementType();
        }
        if (type.getSort() != Type.OBJECT) {
            return;
        }
        //String name = Utils.classPackageToFilePath(type.getClassName());
        String name = type.getClassName();
        if (mClassFilter.test(name)) {
            types.add(name);
        } else if (mCrossModuleFilter.test(name)) {
            crossModuleTypes.add(name);
        }
    }

    public String getSource() {
        return mSource;
    }

    public Set<String> getClassTypes() {
        return mClassTypes;
    }

    private Set<String> getCrossModuleClassTypes() {
        return mCrossModuleClassTypes;
    }

    public Set<Object> getConstantsDefined() {
        return mConstantsDefined;
    }

    public Set<Object> getInlinedUsages() {
        return mInlinedUsages;
    }

    private boolean isAnnotationType(String[] interfaces) {
        return interfaces.length == 1 && interfaces[0].equals("java/lang/annotation/Annotation");
    }

    @Override
    public FieldVisitor visitField(
            int access, String name, String desc, String signature, Object value) {
        maybeAddClassTypesFromSignature(signature, mClassTypes, mCrossModuleClassTypes);
        maybeAddClassType(mClassTypes, mCrossModuleClassTypes, Type.getType(desc));
        if (isAccessibleConstant(access, value)) {
            mConstantsDefined.add(value);
        }
        return new FieldVisitor(mClassTypes, mCrossModuleClassTypes);
    }

    @Override
    public MethodVisitor visitMethod(
            int access, String name, String desc, String signature, String[] exceptions) {
        maybeAddClassTypesFromSignature(signature, mClassTypes, mCrossModuleClassTypes);
        Type methodType = Type.getMethodType(desc);
        maybeAddClassType(mClassTypes, mCrossModuleClassTypes, methodType.getReturnType());
        for (Type argType : methodType.getArgumentTypes()) {
            maybeAddClassType(mClassTypes, mCrossModuleClassTypes, argType);
        }
        return new MethodVisitor(mClassTypes, mCrossModuleClassTypes);
    }

    @Override
    public org.objectweb.asm.AnnotationVisitor visitAnnotation(String desc, boolean visible) {
        if (isAnnotationType && "Ljava/lang/annotation/Retention;".equals(desc)) {
            return mRetentionPolicyVisitor;
        } else {
            maybeAddClassType(mClassTypes, mCrossModuleClassTypes, Type.getType(desc));
            return new AnnotationVisitor(mClassTypes, mCrossModuleClassTypes);
        }
    }

    private static boolean isAccessible(int access) {
        return (access & Opcodes.ACC_PRIVATE) == 0;
    }

    private static boolean isAccessibleConstant(int access, Object value) {
        return isConstant(access) && isAccessible(access) && value != null;
    }

    private static boolean isConstant(int access) {
        return (access & Opcodes.ACC_FINAL) != 0 && (access & Opcodes.ACC_STATIC) != 0;
    }

    public boolean isDependencyToAll() {
        return mIsDependencyToAll;
    }

    private class FieldVisitor extends org.objectweb.asm.FieldVisitor {
        private final Set<String> mTypes;
        private final Set<String> mCrossModuleTypes;

        public FieldVisitor(Set<String> types, Set<String> crossModuleTypes) {
            super(API);
            mTypes = types;
            mCrossModuleTypes = crossModuleTypes;
        }

        @Override
        public org.objectweb.asm.AnnotationVisitor visitAnnotation(
                String descriptor, boolean visible) {
            maybeAddClassType(mTypes, mCrossModuleTypes, Type.getType(descriptor));
            return new AnnotationVisitor(mTypes, mCrossModuleTypes);
        }

        @Override
        public org.objectweb.asm.AnnotationVisitor visitTypeAnnotation(int typeRef,
                TypePath typePath, String descriptor, boolean visible) {
            maybeAddClassType(mTypes, mCrossModuleTypes, Type.getType(descriptor));
            return new AnnotationVisitor(mTypes, mCrossModuleTypes);
        }
    }

    private class MethodVisitor extends org.objectweb.asm.MethodVisitor {
        private final Set<String> mTypes;
        private final Set<String> mCrossModuleTypes;

        protected MethodVisitor(Set<String> types, Set<String> crossModuleTypes) {
            super(API);
            mTypes = types;
            mCrossModuleTypes = crossModuleTypes;
        }

        @Override
        public void visitLdcInsn(Object value) {
            mInlinedUsages.add(value);
            super.visitLdcInsn(value);
        }

        @Override
        public void visitLocalVariable(
                String name, String desc, String signature, Label start, Label end, int index) {
            maybeAddClassTypesFromSignature(signature, mTypes, mCrossModuleClassTypes);
            maybeAddClassType(mTypes, mCrossModuleTypes, Type.getType(desc));
            super.visitLocalVariable(name, desc, signature, start, end, index);
        }

        @Override
        public org.objectweb.asm.AnnotationVisitor visitAnnotation(
                String descriptor, boolean visible) {
            maybeAddClassType(mTypes, mCrossModuleTypes, Type.getType(descriptor));
            return new AnnotationVisitor(mTypes, mCrossModuleTypes);
        }

        @Override
        public org.objectweb.asm.AnnotationVisitor visitParameterAnnotation(
                int parameter, String descriptor, boolean visible) {
            maybeAddClassType(mTypes, mCrossModuleTypes, Type.getType(descriptor));
            return new AnnotationVisitor(mTypes, mCrossModuleTypes);
        }

        @Override
        public org.objectweb.asm.AnnotationVisitor visitTypeAnnotation(
                int typeRef, TypePath typePath, String descriptor, boolean visible) {
            maybeAddClassType(mTypes, mCrossModuleTypes, Type.getType(descriptor));
            return new AnnotationVisitor(mTypes, mCrossModuleTypes);
        }
    }

    private class RetentionPolicyVisitor extends org.objectweb.asm.AnnotationVisitor {
        public RetentionPolicyVisitor() {
            super(ClassDependenciesVisitor.API);
        }

        @Override
        public void visitEnum(String name, String desc, String value) {
            if ("Ljava/lang/annotation/RetentionPolicy;".equals(desc)) {
                RetentionPolicy policy = RetentionPolicy.valueOf(value);
                if (policy == RetentionPolicy.SOURCE) {
                    mIsDependencyToAll = true;
                }
            }
        }
    }

    private class AnnotationVisitor extends org.objectweb.asm.AnnotationVisitor {
        private final Set<String> mTypes;
        private final Set<String> mCrossModuleTypes;

        public AnnotationVisitor(Set<String> types, Set<String> crossModuleTypes) {
            super(ClassDependenciesVisitor.API);
            mTypes = types;
            mCrossModuleTypes = crossModuleTypes;
        }

        @Override
        public void visit(String name, Object value) {
            if (value instanceof Type) {
                maybeAddClassType(mTypes, mCrossModuleTypes, (Type) value);
            }
        }

        @Override
        public org.objectweb.asm.AnnotationVisitor visitArray(String name) {
            return this;
        }

        @Override
        public org.objectweb.asm.AnnotationVisitor visitAnnotation(String name, String descriptor) {
            maybeAddClassType(mTypes, mCrossModuleTypes, Type.getType(descriptor));
            return this;
        }
    }
}