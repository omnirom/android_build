ifeq ($(TARGET_BUILD_APPS),)

.PHONY: systemlicense
systemlicense: $(call corresponding-license-metadata, $(SYSTEM_NOTICE_DEPS)) reportmissinglicenses

ifneq (,$(SYSTEM_NOTICE_DEPS))
SYSTEM_NOTICE_DEPS += $(UNMOUNTED_NOTICE_DEPS) $(UNMOUNTED_NOTICE_VENDOR_DEPS)
$(eval $(call text-notice-rule,$(target_notice_file_txt),"System image",$(system_notice_file_message),$(SYSTEM_NOTICE_DEPS),$(SYSTEM_NOTICE_DEPS)))
endif

.PHONY: vendorlicense
vendorlicense: $(call corresponding-license-metadata, $(VENDOR_NOTICE_DEPS)) reportmissinglicenses

ifneq (,$(VENDOR_NOTICE_DEPS))
VENDOR_NOTICE_DEPS += $(UNMOUNTED_NOTICE_VENDOR_DEPS)
$(eval $(call text-notice-rule,$(target_vendor_notice_file_txt),"Vendor image", \
         "Notices for files contained in all filesystem images except system/system_ext/product/odm/vendor_dlkm/odm_dlkm in this directory:", \
         $(VENDOR_NOTICE_DEPS),$(VENDOR_NOTICE_DEPS)))
endif

.PHONY: odmlicense
odmlicense: $(call corresponding-license-metadata, $(ODM_NOTICE_DEPS)) reportmissinglicenses

ifneq (,$(ODM_NOTICE_DEPS))
$(eval $(call text-notice-rule,$(target_odm_notice_file_txt),"ODM filesystem image", \
         "Notices for files contained in the odm filesystem image in this directory:", \
         $(ODM_NOTICE_DEPS),$(ODM_NOTICE_DEPS)))
endif

.PHONY: oemlicense
oemlicense: $(call corresponding-license-metadata, $(OEM_NOTICE_DEPS)) reportmissinglicenses

.PHONY: productlicense
productlicense: $(call corresponding-license-metadata, $(PRODUCT_NOTICE_DEPS)) reportmissinglicenses

ifneq (,$(PRODUCT_NOTICE_DEPS))
$(eval $(call text-notice-rule,$(target_product_notice_file_txt),"Product image", \
         "Notices for files contained in the product filesystem image in this directory:", \
         $(PRODUCT_NOTICE_DEPS),$(PRODUCT_NOTICE_DEPS)))
endif

.PHONY: systemextlicense
systemextlicense: $(call corresponding-license-metadata, $(SYSTEM_EXT_NOTICE_DEPS)) reportmissinglicenses

ifneq (,$(SYSTEM_EXT_NOTICE_DEPS))
$(eval $(call text-notice-rule,$(target_system_ext_notice_file_txt),"System_ext image", \
         "Notices for files contained in the system_ext filesystem image in this directory:", \
         $(SYSTEM_EXT_NOTICE_DEPS),$(SYSTEM_EXT_NOTICE_DEPS)))
endif

.PHONY: vendor_dlkmlicense
vendor_dlkmlicense: $(call corresponding-license-metadata, $(VENDOR_DLKM_NOTICE_DEPS)) reportmissinglicenses

ifneq (,$(VENDOR_DLKM_NOTICE_DEPS))
$(eval $(call text-notice-rule,$(target_vendor_dlkm_notice_file_txt),"Vendor_dlkm image", \
         "Notices for files contained in the vendor_dlkm filesystem image in this directory:", \
         $(VENDOR_DLKM_NOTICE_DEPS),$(VENDOR_DLKM_NOTICE_DEPS)))
endif

.PHONY: odm_dlkmlicense
odm_dlkmlicense: $(call corresponding-license-metadata, $(ODM_DLKM_NOTICE_DEPS)) reportmissinglicenses

ifneq (,$(ODM_DLKM_NOTICE_DEPS))
$(eval $(call text-notice-rule,$(target_odm_dlkm_notice_file_txt),"ODM_dlkm filesystem image", \
         "Notices for files contained in the odm_dlkm filesystem image in this directory:", \
         $(ODM_DLKM_NOTICE_DEPS),$(ODM_DLKM_NOTICE_DEPS)))
endif

.PHONY: system_dlkmlicense
system_dlkmlicense: $(call corresponding-license-metadata, $(SYSTEM_DLKM_NOTICE_DEPS)) reportmissinglicenses

ifneq (,$(SYSTEM_DLKM_NOTICE_DEPS))
$(eval $(call text-notice-rule,$(target_system_dlkm_notice_file_txt),"System_dlkm filesystem image", \
         "Notices for files contained in the system_dlkm filesystem image in this directory:", \
         $(SYSTEM_DLKM_NOTICE_DEPS),$(SYSTEM_DLKM_NOTICE_DEPS)))
endif

endif # not TARGET_BUILD_APPS
