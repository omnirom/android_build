# Target-specific configuration
ifeq ($(BOARD_USES_QCOM_HARDWARE),true)
    qcom_flags := -DQCOM_HARDWARE
    ifeq ($(TARGET_USES_QCOM_BSP),true)
        qcom_flags += -DQCOM_BSP
        qcom_flags += -DQTI_BSP
    endif

#    TARGET_GLOBAL_CFLAGS += $(qcom_flags)
#    TARGET_GLOBAL_CPPFLAGS += $(qcom_flags)
    PRIVATE_TARGET_GLOBAL_CFLAGS += $(qcom_flags)
    PRIVATE_TARGET_GLOBAL_CPPFLAGS += $(qcom_flags)

    # Multiarch needs these too..
#    2ND_TARGET_GLOBAL_CFLAGS += $(qcom_flags)
#    2ND_TARGET_GLOBAL_CPPFLAGS += $(qcom_flags)
#    2ND_CLANG_TARGET_GLOBAL_CFLAGS += $(qcom_flags)
#    2ND_CLANG_TARGET_GLOBAL_CPPFLAGS += $(qcom_flags)

    TARGET_COMPILE_WITH_MSM_KERNEL := true
    MSM_VIDC_TARGET_LIST := msm8974 msm8610 msm8226 apq8084 msm8916 msm8937 msm8952 msm8953 msm8994 msm8909 msm8992 msm8996 msm8998
endif


