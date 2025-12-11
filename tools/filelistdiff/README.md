# Resolving System Image File List Differences

The Android build system uses the `file_list_diff` tool to ensure consistency
between the lists of installed files in system images defined by Kati and Soong.
This check is crucial when transitioning to Soong-defined system images. If the
tool detects any discrepancies, the build will fail.

This document helps you understand and resolve the reported errors. There are
two main types of errors: files present only in the Kati-defined image
(`Kati only`) and files present only in the Soong-defined image (`Soong only`).

## Understanding and Fixing Errors

### Kati only installed files

This error indicates that certain system modules are included via
`PRODUCT_PACKAGES` in your device's Makefiles (`.mk` files) but are not
explicitly defined within the `android_system_image` module or its default
dependencies in `Android.bp`.

**To resolve this:**

* **Default System Modules:** If the module is defined in a common system
Makefile (like `base_system.mk`, `generic_system.mk`, etc.), ensure it's listed
in the `system_image_defaults` module within
`build/make/target/product/generic/Android.bp`.
* **Device-Specific Modules:** For modules specific to your device, add them to
the relevant `android_system_image` module defined in
`PRODUCT_SOONG_DEFINED_SYSTEM_IMAGE` for your target.

### Soong only installed files

This error means that certain system modules are present in the Soong-defined
system image (specified by `PRODUCT_SOONG_DEFINED_SYSTEM_IMAGE`) but are not
included in the `PRODUCT_PACKAGES` list for your target.

**To resolve this:**

* **Remove Incorrect Modules:** If these modules shouldn't be part of the system
image, remove them from the `android_system_image` module definition.
* **Add Missing Modules to Makefiles:** If these modules are indeed required,
add them to the appropriate `.mk` files, following the guidance in the "Kati
only installed files" section to ensure they are correctly included in both the
Kati and Soong definitions.