// Copyright 2021 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

package main

import (
	"bytes"
	"compress/gzip"
	"flag"
	"fmt"
	"io"
	"io/fs"
	"maps"
	"os"
	"path/filepath"
	"slices"
	"sort"
	"strings"

	"android/soong/response"
	"android/soong/tools/compliance"

	"github.com/google/blueprint/deptools"
)

var (
	failNoneRequested = fmt.Errorf("\nNo license metadata files requested")
	failNoLicenses    = fmt.Errorf("No licenses found")
)

type context struct {
	stdout      io.Writer
	stderr      io.Writer
	rootFS      fs.FS
	product     string
	stripPrefix []string
	title       string
	deps        *[]string
	filter      bool
	filterTo    map[string]struct{}
	replace     []stringPair
}

func (ctx context) postprocessInstallPath(installPath string) string {
	for _, prefix := range ctx.stripPrefix {
		if strings.HasPrefix(installPath, prefix) {
			p := strings.TrimPrefix(installPath, prefix)
			if 0 == len(p) {
				p = ctx.product
			}
			if 0 == len(p) {
				continue
			}
			installPath = p
			break
		}
	}
	for _, replace := range ctx.replace {
		if strings.Contains(installPath, replace.first) {
			installPath = strings.Replace(installPath, replace.first, replace.second, 1)
			break
		}
	}
	return installPath
}

// newMultiString creates a flag that allows multiple values in an array.
func newMultiString(flags *flag.FlagSet, name, usage string) *multiString {
	var f multiString
	flags.Var(&f, name, usage)
	return &f
}

// multiString implements the flag `Value` interface for multiple strings.
type multiString []string

func (ms *multiString) String() string     { return strings.Join(*ms, ", ") }
func (ms *multiString) Set(s string) error { *ms = append(*ms, s); return nil }

// newMultiStringPair creates a flag that allows multiple values in an array.
// Each value must be a pair separated by ":::".
func newMultiStringPair(flags *flag.FlagSet, name, usage string) *multiStringPair {
	var f multiStringPair
	flags.Var(&f, name, usage)
	return &f
}

type stringPair struct {
	first string
	second string
}

// multiString implements the flag `Value` interface for multiple strings.
type multiStringPair []stringPair

func (ms *multiStringPair) String() string {
	var parts []string
	for _, p := range *ms {
		parts = append(parts, p.first + ":::" + p.second)
	}
	return strings.Join(parts, ", ")
}
func (ms *multiStringPair) Set(s string) error {
	parts := strings.Split(s, ":::")
	if len(parts) != 2 {
		return fmt.Errorf("argument must contain exactly 1 \":::\", found: %q", s)
	}
	*ms = append(*ms, stringPair{parts[0], parts[1]})
	return nil
}


// newMultiStringSet creates a flag that allows multiple values in an set.
func newMultiStringSet(flags *flag.FlagSet, name, usage string) *multiStringSet {
	f := make(multiStringSet)
	flags.Var(&f, name, usage)
	return &f
}

// multiString implements the flag `Value` interface for multiple strings.
type multiStringSet map[string]struct{}

func (ms *multiStringSet) String() string {
	keys := slices.Collect(maps.Keys(*ms))
	sort.Strings(keys)
	return strings.Join(keys, ", ")
}
func (ms *multiStringSet) Set(s string) error { (*ms)[s] = struct{}{}; return nil }

func main() {
	var expandedArgs []string
	for _, arg := range os.Args[1:] {
		if strings.HasPrefix(arg, "@") {
			f, err := os.Open(strings.TrimPrefix(arg, "@"))
			if err != nil {
				fmt.Fprintln(os.Stderr, err.Error())
				os.Exit(1)
			}

			respArgs, err := response.ReadRspFile(f)
			f.Close()
			if err != nil {
				fmt.Fprintln(os.Stderr, err.Error())
				os.Exit(1)
			}
			expandedArgs = append(expandedArgs, respArgs...)
		} else {
			expandedArgs = append(expandedArgs, arg)
		}
	}

	flags := flag.NewFlagSet("flags", flag.ExitOnError)

	flags.Usage = func() {
		fmt.Fprintf(os.Stderr, `Usage: %s {options} file.meta_lic {file.meta_lic...}

Outputs a text NOTICE file.

Options:
`, filepath.Base(os.Args[0]))
		flags.PrintDefaults()
	}

	outputFile := flags.String("o", "-", "Where to write the NOTICE text file. (default stdout)")
	depsFile := flags.String("d", "", "Where to write the deps file")
	product := flags.String("product", "", "The name of the product for which the notice is generated.")
	stripPrefix := newMultiString(flags, "strip_prefix", "Prefix to remove from paths. i.e. path to root (multiple allowed)")
	filter := flags.Bool("filter", false, "Enabling flag for filtering. Should be accompanied by filter_to to specify the filter")
	filterTo := newMultiStringSet(flags, "filter_to", "Only list these files in the notice file. Only active if -filter is true. (to allow filtering to empty lists)")
	replace := newMultiStringPair(flags, "replace", "A src:::dst formatted replacement to apply to the filepaths.")
	title := flags.String("title", "", "The title of the notice file.")

	flags.Parse(expandedArgs)

	// Cannot use -filter_to without -filter
	if len(*filterTo) > 0 && !*filter {
		flags.Usage()
		os.Exit(2)
	}

	// Must specify at least one root target.
	if flags.NArg() == 0 {
		flags.Usage()
		os.Exit(2)
	}

	if len(*outputFile) == 0 {
		flags.Usage()
		fmt.Fprintf(os.Stderr, "must specify file for -o; use - for stdout\n")
		os.Exit(2)
	} else {
		dir, err := filepath.Abs(filepath.Dir(*outputFile))
		if err != nil {
			fmt.Fprintf(os.Stderr, "cannot determine path to %q: %s\n", *outputFile, err)
			os.Exit(1)
		}
		fi, err := os.Stat(dir)
		if err != nil {
			fmt.Fprintf(os.Stderr, "cannot read directory %q of %q: %s\n", dir, *outputFile, err)
			os.Exit(1)
		}
		if !fi.IsDir() {
			fmt.Fprintf(os.Stderr, "parent %q of %q is not a directory\n", dir, *outputFile)
			os.Exit(1)
		}
	}

	var ofile io.Writer
	var closer io.Closer
	ofile = os.Stdout
	var obuf *bytes.Buffer
	if *outputFile != "-" {
		obuf = &bytes.Buffer{}
		ofile = obuf
	}
	if strings.HasSuffix(*outputFile, ".gz") {
		ofile, _ = gzip.NewWriterLevel(obuf, gzip.BestCompression)
		closer = ofile.(io.Closer)
	}

	var deps []string

	ctx := &context{
		stdout: ofile,
		stderr: os.Stderr,
		rootFS: compliance.FS,
		product: *product,
		stripPrefix: *stripPrefix,
		title: *title,
		deps: &deps,
		filter: *filter,
		filterTo: *filterTo,
		replace: *replace,
	}

	err := textNotice(ctx, flags.Args()...)
	if err != nil {
		if err == failNoneRequested {
			flags.Usage()
		}
		fmt.Fprintf(os.Stderr, "%s\n", err.Error())
		os.Exit(1)
	}
	if closer != nil {
		closer.Close()
	}

	if *outputFile != "-" {
		err := os.WriteFile(*outputFile, obuf.Bytes(), 0666)
		if err != nil {
			fmt.Fprintf(os.Stderr, "could not write output to %q: %s\n", *outputFile, err)
			os.Exit(1)
		}
	}
	if *depsFile != "" {
		err := deptools.WriteDepFile(*depsFile, *outputFile, deps)
		if err != nil {
			fmt.Fprintf(os.Stderr, "could not write deps to %q: %s\n", *depsFile, err)
			os.Exit(1)
		}
	}
	os.Exit(0)
}

// textNotice implements the textNotice utility.
func textNotice(ctx *context, files ...string) error {
	// Must be at least one root file.
	if len(files) < 1 {
		return failNoneRequested
	}

	// Read the license graph from the license metadata files (*.meta_lic).
	licenseGraph, err := compliance.ReadLicenseGraph(ctx.rootFS, ctx.stderr, files)
	if err != nil {
		return fmt.Errorf("Unable to read license metadata file(s) %q: %v\n", files, err)
	}
	if licenseGraph == nil {
		return failNoLicenses
	}

	// rs contains all notice resolutions.
	rs := compliance.ResolveNotices(licenseGraph)

	ni, err := compliance.IndexLicenseTexts(ctx.rootFS, licenseGraph, rs)
	if err != nil {
		return fmt.Errorf("Unable to read license text file(s) for %q: %v\n", files, err)
	}

	if len(ctx.title) > 0 {
		fmt.Fprintf(ctx.stdout, "%s\n\n", ctx.title)
	}
	for h := range ni.Hashes() {
		if ctx.filter && !ni.ContainsInstall(h, ctx.filterTo) {
			continue
		}
		fmt.Fprintln(ctx.stdout, "==============================================================================")
		for _, libName := range ni.HashLibs(h) {
			if ctx.filter && !ni.ContainsInstallForLib(h, libName, ctx.filterTo) {
				continue
			}
			fmt.Fprintf(ctx.stdout, "%s used by:\n", libName)
			for _, installPath := range ni.HashLibInstalls(h, libName) {
				if _, ok := ctx.filterTo[installPath]; ctx.filter && !ok {
					continue
				}
				fmt.Fprintf(ctx.stdout, "  %s\n", ctx.postprocessInstallPath(installPath))
			}
			fmt.Fprintln(ctx.stdout)
		}
		ctx.stdout.Write(ni.HashText(h))
		fmt.Fprintln(ctx.stdout)
	}

	*ctx.deps = ni.InputFiles()
	sort.Strings(*ctx.deps)

	return nil
}
