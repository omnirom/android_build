#!/usr/bin/env python

from __future__ import print_function

import json
import sys
import os
from xml.etree import ElementTree as ES
try:
	# For python3
	import urllib.request
except ImportError:
	# For python2
	import imp
	import urllib2
	urllib = imp.new_module('urllib')
	urllib.request = urllib2

local_manifest_dir = '.repo/local_manifests'
product = sys.argv[1]

try:
	device = product[product.index("_") + 1:]
except:
	device = product


def check_repo_exists(git_data):
	if not int(git_data.get('total_count', 0)):
		print("{} 1 not found in OmniRom Github, exiting roomservice".format(device))
		sys.exit(1)


# note that this can only be done 5 times per minute
def search_github_for_device(device):
	git_search_url = "https://api.github.com/search/repositories?q=%40OmniRom+android_device+{}"
	git_req = urllib.request.Request(git_search_url.format(device))
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
	print("{} not found in OmniRom Github, exiting roomservice".format(device))
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
			lm = ES.parse(local_manifest_dir + "/" + file)
			lm = lm.getroot()

			for project in lm.findall("project"):
				if project.get("name") == url:
					return True
		except:
			print("WARNING: error while parsing %s/%s. This will cause repo sync errors.."
					% (local_manifest_dir, file))

	lm = ES.parse(".repo/manifest.xml")
	lm = lm.getroot()
	for project in lm.findall("project"):
		if project.get("name") == url:
			return True

	return False


# use the indent function from http://stackoverflow.com/a/4590052
def indent(elem, level=0):
	i = "\n" + level*"  "
	if len(elem):
		if not elem.text or not elem.text.strip():
			elem.text = i + "  "
		if not elem.tail or not elem.tail.strip():
			elem.tail = i
		for elem in elem:
			indent(elem, level+1)
		if not elem.tail or not elem.tail.strip():
			elem.tail = i
	else:
		if level and (not elem.tail or not elem.tail.strip()):
			elem.tail = i


def create_manifest_project(device_url, device_dir,
		remote="github", revision="android-4.3"):
	# first check if any of the current manifests contain the projects we're fetching
	project_exists = check_project_exists(device_url)

	if project_exists:
		return 0

	project = ES.Element("project", attrib = { "path": device_dir, 
		"name": device_url, "remote": remote, "revision": revision})
	return project


def append_to_manifest(project):
	try:
		lm = ES.parse(local_manifest_dir + "/roomservice.xml")
		lm = lm.getroot()
	except:
		lm = ES.Element("manifest")
	lm.append(project)
	return lm

def write_to_manifest(manifest):
	indent(manifest)
	raw_xml = ES.tostring(manifest).decode()
	raw_xml = '<?xml version="1.0" encoding="UTF-8"?>\n' + raw_xml

	with open(local_manifest_dir + "/roomservice.xml", 'w') as f:
		f.write(raw_xml)
	print("written the new roomservice manifest")

git_data = search_github_for_device(device)
device_url = get_device_url(git_data)
device_dir = parse_device_directory(device_url)
project = create_manifest_project(device_url, device_dir, remote="omnirom")
if not project == 0:
	manifest = append_to_manifest(project)
	write_to_manifest(manifest)
else:
	print("project already added to roomservice.xml")

print("syncing the device config")
os.system('repo sync %s' % device_dir)
