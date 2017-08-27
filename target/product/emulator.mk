#
# Copyright (C) 2012 The Android Open Source Project
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#      http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

#
# This file is included by other product makefiles to add all the
# emulator-related modules to PRODUCT_PACKAGES.
#

# Host modules
PRODUCT_PACKAGES += \


# Device modules
PRODUCT_PACKAGES += \
    egl.cfg \
    gralloc.goldfish \
    gralloc.ranchu \
    libGLESv1_CM_emulation \
    lib_renderControl_enc \
    libEGL_emulation \
    libGLES_android \
    libGLESv2_enc \
    libOpenglSystemCommon \
    libGLESv2_emulation \
    libGLESv1_enc \
    qemu-props \
    qemud \
    camera.goldfish \
    camera.goldfish.jpeg \
    camera.ranchu \
    camera.ranchu.jpeg \
    lights.goldfish \
    gps.goldfish \
    gps.ranchu \
    fingerprint.goldfish \
    sensors.goldfish \
    audio.primary.goldfish \
    audio.primary.goldfish_legacy \
    android.hardware.audio@2.0-service \
    vibrator.goldfish \
    power.goldfish \
    power.ranchu \
    fingerprint.ranchu \
    android.hardware.biometrics.fingerprint@2.1-service \
    sensors.ranchu \
    android.hardware.graphics.composer@2.1-impl \
    android.hardware.graphics.composer@2.1-service \
    hwcomposer.goldfish \
    hwcomposer.ranchu \
    android.hardware.graphics.allocator@2.0-impl \
    android.hardware.graphics.allocator@2.0-service \
    android.hardware.graphics.mapper@2.0-impl

PRODUCT_PACKAGES += \
    android.hardware.audio@2.0-impl \
    android.hardware.audio.effect@2.0-impl \
    android.hardware.broadcastradio@1.0-impl \
    android.hardware.soundtrigger@2.0-impl

PRODUCT_PACKAGES += \
    android.hardware.keymaster@3.0-impl \
    android.hardware.keymaster@3.0-service

PRODUCT_PACKAGES += \
    android.hardware.gnss@1.0-service \
    android.hardware.gnss@1.0-impl

PRODUCT_PACKAGES += \
    android.hardware.sensors@1.0-impl \
    android.hardware.sensors@1.0-service

PRODUCT_PACKAGES += \
    android.hardware.power@1.0-service \
    android.hardware.power@1.0-impl

# camera service treble disable until all backwards compat is complete
PRODUCT_PROPERTY_OVERRIDES += \
    camera.disable_treble=1

PRODUCT_COPY_FILES += \
    device/generic/goldfish/fstab.goldfish:root/fstab.goldfish \
    device/generic/goldfish/init.goldfish.rc:root/init.goldfish.rc \
    device/generic/goldfish/init.goldfish.sh:system/etc/init.goldfish.sh \
    device/generic/goldfish/ueventd.goldfish.rc:root/ueventd.goldfish.rc \
    device/generic/goldfish/init.ranchu.rc:root/init.ranchu.rc \
    device/generic/goldfish/fstab.ranchu:root/fstab.ranchu \
    device/generic/goldfish/ueventd.ranchu.rc:root/ueventd.ranchu.rc \
    device/generic/goldfish/input/goldfish_rotary.idc:system/usr/idc/goldfish_rotary.idc \
    frameworks/native/data/etc/android.hardware.usb.accessory.xml:system/etc/permissions/android.hardware.usb.accessory.xml

PRODUCT_COPY_FILES += \
    device/generic/goldfish/manifest.xml:system/vendor/manifest.xml \
    device/generic/goldfish/init.ranchu-core.sh:system/vendor/bin/init.ranchu-core.sh \
    device/generic/goldfish/init.ranchu-net.sh:system/vendor/bin/init.ranchu-net.sh


PRODUCT_PACKAGE_OVERLAYS := device/generic/goldfish/overlay

PRODUCT_CHARACTERISTICS := emulator
