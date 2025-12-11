#
# Copyright (C) 2022 The Android Open Source Project
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

import os.path
import shutil

import common
import merge_compatibility_checks
import merge_target_files
import test_utils


class MergeCompatibilityChecksTest(test_utils.ReleaseToolsTestCase):

  def setUp(self):
    self.testdata_dir = test_utils.get_testdata_dir()
    self.partition_map = {
        'system': 'system',
        'system_ext': 'system_ext',
        'product': 'product',
        'vendor': 'vendor',
        'odm': 'odm',
    }
    self.OPTIONS = merge_target_files.OPTIONS
    self.OPTIONS.framework_partition_set = set(
        ['product', 'system', 'system_ext'])
    self.OPTIONS.vendor_partition_set = set(['odm', 'vendor'])

  def test_CheckCombinedSepolicy(self):
    product_out_dir = common.MakeTempDir()

    def write_temp_file(path, data=''):
      full_path = os.path.join(product_out_dir, path)
      if not os.path.exists(os.path.dirname(full_path)):
        os.makedirs(os.path.dirname(full_path))
      with open(full_path, 'w') as f:
        f.write(data)

    write_temp_file(
        'system/etc/vintf/compatibility_matrix.device.xml', """
      <compatibility-matrix>
        <sepolicy>
          <kernel-sepolicy-version>30</kernel-sepolicy-version>
        </sepolicy>
      </compatibility-matrix>""")
    write_temp_file('vendor/etc/selinux/plat_sepolicy_vers.txt', '202604')
    write_temp_file('vendor/etc/selinux/genfs_labels_version.txt', '202604')
    write_temp_file('system/build.prop', """
                    # test build.prop
                    ro.debuggable=1
                    # end test build.prop
                    """)

    write_temp_file('system/etc/selinux/plat_sepolicy.cil')
    write_temp_file('system/etc/selinux/mapping/202604.cil')
    write_temp_file('system/etc/selinux/plat_sepolicy_genfs_202604.cil')
    write_temp_file('system/etc/selinux/plat_seapp_contexts')
    write_temp_file('product/etc/selinux/mapping/202604.cil')
    write_temp_file('product/etc/selinux/product_seapp_contexts')
    write_temp_file('vendor/etc/selinux/vendor_sepolicy.cil')
    write_temp_file('vendor/etc/selinux/plat_pub_versioned.cil')
    write_temp_file('vendor/etc/selinux/vendor_file_contexts')
    write_temp_file('vendor/etc/selinux/vendor_seapp_contexts')

    write_temp_file('system/app/Settings/Settings.apk')
    write_temp_file('system_ext/app/SystemUI/SystemUI.apk')
    write_temp_file('product/priv-app/Camera/Camera.apk')
    write_temp_file('vendor/app/VendorApp/VendorApp.apk')

    cmds = merge_compatibility_checks.CheckCombinedSepolicy(
        product_out_dir, self.partition_map, execute=False)

    self.assertEqual(' '.join(cmds[0]),
                     ('secilc -m -M true -G -N -c 30 '
                      '-o {OTP}/META/combined_sepolicy -f /dev/null '
                      '{OTP}/system/etc/selinux/plat_sepolicy.cil '
                      '{OTP}/system/etc/selinux/mapping/202604.cil '
                      '{OTP}/vendor/etc/selinux/vendor_sepolicy.cil '
                      '{OTP}/vendor/etc/selinux/plat_pub_versioned.cil '
                      '{OTP}/product/etc/selinux/mapping/202604.cil '
                      '{OTP}/system/etc/selinux/plat_sepolicy_genfs_202604.cil').format(
                          OTP=product_out_dir))

    temp_dirs = list(filter(lambda dir: 'treble_labeling_tests_' in dir, self.OPTIONS.tempfiles))
    self.assertEqual(len(temp_dirs), 1)
    temp_dir = temp_dirs[0]

    with open(os.path.join(temp_dir, 'platform_apps.txt'), 'r') as f:
      platform_apps = f.read()
    self.assertEqual(platform_apps,
                     (f'{product_out_dir}/system/app/Settings/Settings.apk\n'
                      f'{product_out_dir}/system_ext/app/SystemUI/SystemUI.apk\n'
                      f'{product_out_dir}/product/priv-app/Camera/Camera.apk'))

    with open(os.path.join(temp_dir, 'vendor_apps.txt'), 'r') as f:
      vendor_apps = f.read()
    self.assertEqual(vendor_apps, f'{product_out_dir}/vendor/app/VendorApp/VendorApp.apk')

    self.assertEqual(' '.join(cmds[1]),
                     ('secilc -m -M true -G -N -c 30 '
                      '-o {TEMP}/platform_sepolicy -f /dev/null '
                      '{OTP}/system/etc/selinux/plat_sepolicy.cil').format(
                          OTP=product_out_dir, TEMP=temp_dir))

    old_max_diff = self.maxDiff
    self.maxDiff = None # for long diff
    self.assertEqual(' '.join(cmds[2]),
                     ('treble_labeling_tests --platform_apks {TEMP}/platform_apps.txt '
                      '--vendor_apks {TEMP}/vendor_apps.txt '
                      '--precompiled_sepolicy_without_vendor {TEMP}/platform_sepolicy '
                      '--precompiled_sepolicy {OTP}/META/combined_sepolicy '
                      '--platform_seapp_contexts '
                      '{OTP}/system/etc/selinux/plat_seapp_contexts '
                      '{OTP}/product/etc/selinux/product_seapp_contexts '
                      '--vendor_seapp_contexts '
                      '{OTP}/vendor/etc/selinux/vendor_seapp_contexts '
                      '--vendor_file_contexts '
                      '{OTP}/vendor/etc/selinux/vendor_file_contexts '
                      '--aapt2_path aapt2 --treat_as_warnings --debuggable').format(
                          OTP=product_out_dir, TEMP=temp_dir))
    self.maxDiff = old_max_diff

  def _copy_apex(self, source, output_dir, partition):
    shutil.copy(
        source,
        os.path.join(output_dir, partition, 'apex', os.path.basename(source)))

  @test_utils.SkipIfExternalToolsUnavailable()
  def test_CheckApexDuplicatePackages(self):
    output_dir = common.MakeTempDir()
    os.makedirs(os.path.join(output_dir, 'SYSTEM/apex'))
    os.makedirs(os.path.join(output_dir, 'VENDOR/apex'))

    self._copy_apex(
        os.path.join(self.testdata_dir, 'has_apk.apex'), output_dir, 'SYSTEM')
    self._copy_apex(
        os.path.join(test_utils.get_current_dir(),
                     'com.android.apex.compressed.v1.capex'), output_dir,
        'VENDOR')
    self.assertEqual(
        len(
            merge_compatibility_checks.CheckApexDuplicatePackages(
                output_dir, self.partition_map)), 0)

  @test_utils.SkipIfExternalToolsUnavailable()
  def test_CheckApexDuplicatePackages_RaisesOnPackageInMultiplePartitions(self):
    output_dir = common.MakeTempDir()
    os.makedirs(os.path.join(output_dir, 'SYSTEM/apex'))
    os.makedirs(os.path.join(output_dir, 'VENDOR/apex'))

    same_apex_package = os.path.join(self.testdata_dir, 'has_apk.apex')
    self._copy_apex(same_apex_package, output_dir, 'SYSTEM')
    self._copy_apex(same_apex_package, output_dir, 'VENDOR')
    self.assertEqual(
        merge_compatibility_checks.CheckApexDuplicatePackages(
            output_dir, self.partition_map)[0],
        'Duplicate APEX package_names found in multiple partitions: com.android.wifi'
    )
