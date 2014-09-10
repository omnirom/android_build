# Configuration for Linux on ARM.
# Generating binaries for the ARMv7-a architecture and higher with NEON
#
ARCH_ARM_HAVE_ARMV7A            := true
ARCH_ARM_HAVE_TLS_REGISTER      := true
ARCH_ARM_HAVE_VFP               := true
ARCH_ARM_HAVE_VFP_D32           := true
ARCH_ARM_HAVE_NEON              := true

ifeq ($(strip $(TARGET_CPU_VARIANT)),cortex-a15)
	arch_variant_cflags := -mcpu=cortex-a15 -mfpu=neon-vfpv4
else
ifeq ($(strip $(TARGET_CPU_VARIANT)),cortex-a9)
	arch_variant_cflags := -mcpu=cortex-a9 -mfpu=neon
else
ifeq ($(strip $(TARGET_CPU_VARIANT)),cortex-a8)
	arch_variant_cflags := -mcpu=cortex-a8 -mfpu=neon
else
ifeq ($(strip $(TARGET_CPU_VARIANT)),cortex-a7)
	arch_variant_cflags := -mcpu=cortex-a7 -mfpu=neon-vfpv4
else
ifeq ($(strip $(TARGET_CPU_VARIANT)),cortex-a5)
	arch_variant_cflags := -mcpu=cortex-a5 -mfpu=neon-vfpv4
else
ifeq ($(strip $(TARGET_CPU_VARIANT)),krait)
	arch_variant_cflags := -mcpu=cortex-a9
else
ifeq ($(strip $(TARGET_CPU_VARIANT)),scorpion)
	arch_variant_cflags := -mcpu=cortex-a8 -mfpu=neon
else
	arch_variant_cflags := -march=armv7-a -mfpu=neon
endif
endif
endif
endif
endif
endif
endif

arch_variant_cflags += \
    -mfloat-abi=softfp

arch_variant_ldflags := \
	-Wl,--fix-cortex-a8
