#!/usr/bin/env python
# Copyright (C) 2013 The OmniROM Project
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <http://www.gnu.org/licenses/>.

from __future__ import print_function
import json
import sys
import os
from xml.etree import ElementTree as ES
# Use the urllib importer from the Cyanogenmod roomservice
try:
    # For python3
    import urllib.request
except ImportError:
    # For python2
    import imp
    import urllib2
    urllib = imp.new_module('urllib')
    urllib.request = urllib2

default_rem        = "github"
default_rev        = "android-4.3"
local_manifest_dir = '.repo/local_manifests'
product            = sys.argv[1]
if len(sys.argv) > 2:
    deps_only      = sys.argv[2]
else:
    deps_only      = False

try:
    device         = product[product.index("_") + 1:]
except:
    device         = product


def check_repo_exists(git_data):
    if not int(git_data.get('total_count', 0)):
        print("{} not found in OmniRom Github, exiting " \
              "roomservice".format(device))
        sys.exit(1)


# Note that this can only be done 5 times per minute
def search_github_for_device(device):
    git_search_url = "https://api.github.com/search/repositories" \
                     "?q=%40OmniRom+android_device+{}".format(device)
    git_req = urllib.request.Request(git_search_url)
    # this api is a preview at the moment. accept the custom media type
    git_req.add_header('Accept', 'application/vnd.github.preview')
    response = urllib.request.urlopen(git_req)
    git_data = json.load(response)
    check_repo_exists(git_data)
    print("found the {} device repo".format(device))
    return git_data



def get_device_url(git_data):
    device_url = ""
    for item in git_data['items']:
        temp_url = item.get('html_url')
        try:
            temp_url = temp_url[temp_url.index("android_device"):]
            if temp_url.endswith(device):
                device_url = temp_url
                break
        except:
            pass
    if device_url:
        return device_url
    print("{} not found in OmniRom Github, exiting " \
          "roomservice".format(device))
    sys.exit(1)


def parse_device_directory(device_url):
    to_strip = "android_device"
    repo_name = device_url[device_url.index(to_strip) + len(to_strip):]
    repo_dir = repo_name.replace("_", "/")
    return "device{}".format(repo_dir)


def check_project_exists(url):
    # check all the local manifests
    for file in os.listdir(local_manifest_dir):
        try:
            lm = ES.parse('/'.join([local_manifest_dir, file]))
            lm = lm.getroot()

            for project in lm.findall("project"):
                if project.get("name") == url:
                    return True
        except:
            print("WARNING: error while parsing %s/%s. " \
                  "This will cause repo sync errors.."
                      % (local_manifest_dir, file))

    lm = ES.parse(".repo/manifest.xml")
    lm = lm.getroot()
    for project in lm.findall("project"):
        if project.get("name") == url:
            return True

    return False


# Use the indent function from http://stackoverflow.com/a/4590052
def indent(elem, level=0):
    i = ''.join(["\n", level*"  "])
    if len(elem):
        if not elem.text or not elem.text.strip():
            elem.text = ''.join([i, "  "])
        if not elem.tail or not elem.tail.strip():
            elem.tail = i
        for elem in elem:
            indent(elem, level+1)
        if not elem.tail or not elem.tail.strip():
            elem.tail = i
    else:
        if level and (not elem.tail or not elem.tail.strip()):
            elem.tail = i


def create_manifest_project(url, directory,
                            remote=default_rem,
                            revision=default_rev):
    project_exists = check_project_exists(url)

    if project_exists:
        return None

    project = ES.Element("project",
                         attrib = {
                                   "path": directory,
                                   "name": url,
                                   "remote": remote,
                                   "revision": revision
                                  })
    return project


def append_to_manifest(project):
    try:
        lm = ES.parse('/'.join([local_manifest_dir, "roomservice.xml"]))
        lm = lm.getroot()
    except:
        lm = ES.Element("manifest")
    lm.append(project)
    return lm


def write_to_manifest(manifest):
    indent(manifest)
    raw_xml = ES.tostring(manifest).decode()
    raw_xml = ''.join(['<?xml version="1.0" encoding="UTF-8"?>\n', raw_xml])

    with open('/'.join([local_manifest_dir, "roomservice.xml"]), 'w') as f:
        f.write(raw_xml)
    print("written the new roomservice manifest")


def parse_device_from_manifest(device):
    for file in os.listdir(local_manifest_dir):
        try:
            lm = ES.parse('/'.join([local_manifest_dir, file]))
            lm = lm.getroot()

            for project in lm.findall("project"):
                name = project.get("name")
                if name.startswith("android_device_") \
                   and name.endswith(device):
                    return project.get("path")
        except:
            pass

    lm = ES.parse(".repo/manifest.xml")
    lm = lm.getroot()
    for project in lm.findall("project"):
        if name.startswith("android_device_") and name.endswith(device):
            return project.get("path")

    return None

def parse_device_from_folder(device):
    search = []
    for sub_folder in os.listdir("device"):
        if os.path.isdir("device/%s/%s" % (sub_folder, device)):
            search.append("device/%s/%s" % (sub_folder, device))
    if len(search) > 1:
        print("multiple devices under the name %s. " \
              "defaulting to checking the manifest" % device)
        location = parse_device_from_manifest(device)
    else:
        location = search[0]
    return location


def parse_dependency_file(location):
    dep_file = "omni.dependencies"
    dep_location = '/'.join([location, dep_file])
    if not os.path.isfile(dep_location):
        print("WARNING: %s file not found" % dep_location)
        sys.exit()
    try:
        with open(dep_location, 'r') as f:
            dependencies = json.loads(f.read())
    except:
        print("ERROR: malformed dependency file")
    return dependencies


def create_dependency_manifest(dependencies):
    for dependie in dependencies:
        repository  = dependie.get("repository")
        target_path = dependie.get("target_path")
        revision    = dependie.get("revision", default_rev)
        remote      = dependie.get("remote", default_rem)

        # not adding an organization should default to omnirom
        # only apply this to github
        if remote == "github":
            if not "/" in repository:
                repository = '/'.join(["omnirom", repository])
        project = create_manifest_project(repository,
                                          target_path,
                                          remote=remote,
                                          revision=revision)
        if not project is None:
            manifest = append_to_manifest(project)
            write_to_manifest(manifest)
            os.system("repo sync %s" % target_path)


def fetch_dependencies(device):
    location = parse_device_from_folder(device)
    if not os.path.isdir(location):
        print("ERROR: could not find your device " \
              "folder location, bailing out")
        sys.exit()
    dependencies = parse_dependency_file(location)
    create_dependency_manifest(dependencies)

def fetch_device(device):
    git_data = search_github_for_device(device)
    device_url = get_device_url(git_data)
    device_dir = parse_device_directory(device_url)
    project = create_manifest_project(device_url,
                                      device_dir,
                                      remote="omnirom")
    if not project is None:
        manifest = append_to_manifest(project)
        write_to_manifest(manifest)
        print("syncing the device config")
        os.system('repo sync %s' % device_dir)
    fetch_dependencies(device)

if deps_only:
    fetch_dependencies(device)
else:
    fetch_device(device)
