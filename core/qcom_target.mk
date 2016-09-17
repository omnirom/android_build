# Target-specific configuration
ifeq ($(BOARD_USES_QCOM_HARDWARE),true)
    qcom_flags := -DQCOM_HARDWARE
    ifeq ($(TARGET_USES_QCOM_BSP),true)
        qcom_flags += -DQCOM_BSP
        qcom_flags += -DQTI_BSP
    endif

    TARGET_GLOBAL_CFLAGS += $(qcom_flags)
    TARGET_GLOBAL_CPPFLAGS += $(qcom_flags)
    CLANG_TARGET_GLOBAL_CFLAGS += $(qcom_flags)
    CLANG_TARGET_GLOBAL_CPPFLAGS += $(qcom_flags)

    # Multiarch needs these too..
    2ND_TARGET_GLOBAL_CFLAGS += $(qcom_flags)
    2ND_TARGET_GLOBAL_CPPFLAGS += $(qcom_flags)
    2ND_CLANG_TARGET_GLOBAL_CFLAGS += $(qcom_flags)
    2ND_CLANG_TARGET_GLOBAL_CPPFLAGS += $(qcom_flags)
endif

