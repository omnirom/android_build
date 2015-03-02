
-include build/core/tasks/kernel.mk

ifneq($(TARGET_NO_BOOTLOADER),true)
ifeq ($(BOARD_USES_UBOOT),true)
ifneq ($(strip $(TARGET_BOOTLOADER_CONFIG)),)

TARGET_BOOTLOADER_IMAGE := $(PRODUCT_OUT)/u-boot.bin

BOOTLOADER_PATH := bootable/bootloader/uboot/
BOOTLOADER_CONFIG_FILE := $(BOOTLOADER_PATH)/include/config.h

ifeq ($(TARGET_ARCH),arm)
BOOTLOADER_ENV := ARCH=$(TARGET_ARCH) $(ARM_CROSS_COMPILE)
else
BOOTLOADER_ENV := ARCH=$(TARGET_ARCH)
endif # TARGET_ARCH

$(TARGET_BOOTLOADER_IMAGE):
	@for ubootplat in $(TARGET_BOOTLOADER_CONFIG); do \
		UBOOT_PLATFORM=`echo $$ubootplat | cut -d':' -f1`; \
		UBOOT_CONFIG=`echo $$ubootplat | cut -d':' -f2`; \
		$(MAKE) $(MAKE_FLAGS) -C $(BOOTLOADER_PATH) distclean $(BOOTLOADER_ENV); \
		$(MAKE) $(MAKE_FLAGS) -C $(BOOTLOADER_PATH) $$UBOOT_CONFIG $(BOOTLOADER_ENV); \
		$(MAKE) $(MAKE_FLAGS) -C $(BOOTLOADER_PATH) $(BOOTLOADER_ENV); \
		install -D $(BOOTLOADER_PATH)/u-boot.bin $(PRODUCT_OUT)/u-boot-$$UBOOT_PLATFORM.bin; \
		install -D $(BOOTLOADER_PATH)/u-boot.bin $@; \
	done

endif # TARGET_BOOTLOADER_CONFIG
endif # BOARD_USES_UBOOT
endif # TARGET_NO_BOOTLOADER
