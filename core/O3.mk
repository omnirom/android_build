# Copyright (C) 2014-2015 The SaberMod Project
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

ifneq (1,$(words $(filter $(LOCAL_DISABLE_O3),$(LOCAL_MODULE))))
  LOCAL_ARM_MODE := $(strip $(LOCAL_ARM_MODE))
  ifeq (arm,$(TARGET_$(LOCAL_2ND_ARCH_VAR_PREFIX)ARCH))
    arm_objects_mode := $(if $(LOCAL_ARM_MODE),$(LOCAL_ARM_MODE),arm)
    normal_objects_mode := $(if $(LOCAL_ARM_MODE),$(LOCAL_ARM_MODE),thumb)

    # Read the values from something like TARGET_arm_CFLAGS or
    # TARGET_thumb_CFLAGS.  HOST_(arm|thumb)_CFLAGS values aren't
    # actually used (although they are usually empty).
    arm_objects_cflags := $($(LOCAL_2ND_ARCH_VAR_PREFIX)$(my_prefix)$(arm_objects_mode)_CFLAGS)
    normal_objects_cflags := $($(LOCAL_2ND_ARCH_VAR_PREFIX)$(my_prefix)$(normal_objects_mode)_CFLAGS)
  else
    arm_objects_mode :=
    normal_objects_mode :=
    arm_objects_cflags :=
    normal_objects_cflags :=
  endif
  ifeq ($(filter $(LOCAL_DISABLE_O3_CFLAGS),$(arm_objects_cflags)$(normal_objects_cflags)),)
    ifdef LOCAL_CONLYFLAGS
      LOCAL_CONLYFLAGS += $(O3_FLAGS)
    else
      LOCAL_CONLYFLAGS := $(O3_FLAGS)
    endif
    ifdef LOCAL_CPPFLAGS
      LOCAL_CPPFLAGS += $(O3_FLAGS)
    else
      LOCAL_CPPFLAGS := $(O3_FLAGS)
    endif
  endif
endif
