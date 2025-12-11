use crate::codegen::CodegenMode;
use crate::dump::DumpFormat;
use aconfig_storage_file::{StorageFileType, DEFAULT_FILE_VERSION, MAX_SUPPORTED_FILE_VERSION};

use anyhow::{anyhow, bail, ensure, Context, Result};
use clap::{builder::ArgAction, builder::EnumValueParser, Arg, ArgMatches, Command};
use core::any::Any;
use std::ffi::OsString;
use std::io::BufRead;
use std::path::PathBuf;

const HELP_DUMP_CACHE: &str = r#"
An aconfig cache file, created via `aconfig create-cache`.
"#;

const HELP_DUMP_FORMAT: &str = r#"
Change the output format for each flag.

The argument to --format is a format string. Each flag will be a copy of this string, with certain
placeholders replaced by attributes of the flag. The placeholders are

  {package}
  {name}
  {namespace}
  {description}
  {bug}
  {state}
  {state:bool}
  {permission}
  {trace}
  {trace:paths}
  {is_fixed_read_only}
  {is_exported}
  {container}
  {metadata}
  {fully_qualified_name}

Note: the format strings "textproto" and "protobuf" are handled in a special way: they output all
flag attributes in text or binary protobuf format.

Examples:

  # See which files were read to determine the value of a flag; the files were read in the order
  # listed.
  --format='{fully_qualified_name} {trace}'

  # Trace the files read for a specific flag. Useful during debugging.
  --filter=fully_qualified_name:com.foo.flag_name --format='{trace}'

  # Print a somewhat human readable description of each flag.
  --format='The flag {name} in package {package} is {state} and has permission {permission}.'
"#;

const HELP_DUMP_FILTER: &str = r#"
Limit which flags to output. If --filter is omitted, all flags will be printed. If multiple
--filter options are provided, the output will be limited to flags that match any of the filters.

The argument to --filter is a search query. Multiple queries can be AND-ed together by
concatenating them with a plus sign.

Valid queries are:

  package:<string>
  name:<string>
  namespace:<string>
  bug:<string>
  state:ENABLED|DISABLED
  permission:READ_ONLY|READ_WRITE
  is_fixed_read_only:true|false
  is_exported:true|false
  container:<string>
  fully_qualified_name:<string>

Note: there is currently no support for filtering based on these flag attributes: description,
trace, metadata.

Examples:

  # Print a single flag:
  --filter=fully_qualified_name:com.foo.flag_name

  # Print all known information about a single flag:
  --filter=fully_qualified_name:com.foo.flag_name --format=textproto

  # Print all flags in the com.foo package, and all enabled flags in the com.bar package:
  --filter=package:com.foo --filter=package.com.bar+state:ENABLED
"#;

const HELP_DUMP_DEDUP: &str = r#"
Allow the same flag to be present in multiple cache files; if duplicates are found, collapse into
a single instance.
"#;

const MAINLINE_BETA_NAMESPACE_CONFIG: &str = r#"
A json file to configure mainline beta namespaces. This option is internal to Google. The json
configuration should assume the following format:

{
    "namespaces": {
        "com_android_tethering": {
            "container": "com.android.tethering",
            "allow_exported": true
        },
        "com_android_mediaprovider": {
            "container": "com.android.mediaprovider",
            "allow_exported": true
        }
    }
}
"#;

/// Conventional prefix to mark response file
pub const RESPONSE_FILE_PREFIX: char = '@';

// Trait for Reading Response Files
// Defines the capability to read lines from a response file path.
// Allows mocking file access during testing.
pub trait ResponseFileReader {
    fn read_to_bufread(&self, path_str: &str) -> Result<Box<dyn BufRead>>;
}

#[derive(Debug)]
pub enum ParsedCommand {
    CreateCache {
        package: String,
        container: String,
        declarations: Vec<String>,
        values: Vec<String>,
        default_permission: aconfig_protos::ProtoFlagPermission,
        allow_read_write: bool,
        cache_out_path: String,
        mainline_beta_namespace_config: Option<PathBuf>,
        force_read_only: bool,
    },
    CreateJavaLib {
        cache_path: String,
        out_dir: PathBuf,
        mode: CodegenMode,
        single_exported_file: bool,
    },
    CreateCppLib {
        cache_path: String,
        out_dir: PathBuf,
        mode: CodegenMode,
    },
    CreateRustLib {
        cache_path: String,
        out_dir: PathBuf,
        mode: CodegenMode,
    },
    DumpCache {
        cache_paths: Vec<String>,
        format: DumpFormat,
        filters: Vec<String>,
        dedup: bool,
        out_path: String,
    },
    CreateStorage {
        container: String,
        file_type: StorageFileType,
        cache_paths: Vec<String>,
        out_path: String,
        version: u32,
    },
}

fn build_cli() -> Command {
    Command::new("aconfig")
        .subcommand_required(true)
        .about(format!("A tool trunk flags. Supports {RESPONSE_FILE_PREFIX}responsefile syntax."))
        .subcommand(
            Command::new("create-cache")
                .arg(Arg::new("package").long("package").required(true))
                .arg(Arg::new("container").long("container").required(true))
                .arg(Arg::new("declarations").long("declarations").action(ArgAction::Append))
                .arg(Arg::new("values").long("values").action(ArgAction::Append))
                .arg(
                    Arg::new("default-permission")
                        .long("default-permission")
                        .value_parser(aconfig_protos::flag_permission::parse_from_str)
                        .default_value(aconfig_protos::flag_permission::to_string(
                            &crate::commands::DEFAULT_FLAG_PERMISSION,
                        )),
                )
                .arg(
                    Arg::new("allow-read-write")
                        .long("allow-read-write")
                        .value_parser(clap::value_parser!(bool))
                        .default_value("true"),
                )
                .arg(Arg::new("cache").long("cache").required(true).help("Output cache file path."))
                .arg(
                    Arg::new("mainline-beta-namespace-config")
                        .long("mainline-beta-namespace-config")
                        .long_help(MAINLINE_BETA_NAMESPACE_CONFIG.trim()),
                )
                .arg(
                    Arg::new("force-read-only")
                        .long("force-read-only")
                        .value_parser(clap::value_parser!(bool))
                        .default_value("false"),
                ),
        )
        .subcommand(
            Command::new("create-java-lib")
                .arg(Arg::new("cache").long("cache").required(true))
                .arg(Arg::new("out").long("out").required(true))
                .arg(
                    Arg::new("mode")
                        .long("mode")
                        .value_parser(EnumValueParser::<CodegenMode>::new())
                        .default_value("production"),
                )
                .arg(
                    Arg::new("single-exported-file")
                        .long("single-exported-file")
                        .value_parser(clap::value_parser!(bool))
                        .default_value("false"),
                ),
        )
        .subcommand(
            Command::new("create-cpp-lib")
                .arg(Arg::new("cache").long("cache").required(true))
                .arg(Arg::new("out").long("out").required(true))
                .arg(
                    Arg::new("mode")
                        .long("mode")
                        .value_parser(EnumValueParser::<CodegenMode>::new())
                        .default_value("production"),
                ),
        )
        .subcommand(
            Command::new("create-rust-lib")
                .arg(Arg::new("cache").long("cache").required(true))
                .arg(Arg::new("out").long("out").required(true))
                .arg(
                    Arg::new("mode")
                        .long("mode")
                        .value_parser(EnumValueParser::<CodegenMode>::new())
                        .default_value("production"),
                ),
        )
        .subcommand(
            Command::new("dump-cache")
                .alias("dump")
                .arg(
                    Arg::new("cache")
                        .long("cache")
                        .action(ArgAction::Append)
                        .long_help(HELP_DUMP_CACHE.trim()),
                )
                .arg(
                    Arg::new("format")
                        .long("format")
                        .value_parser(|s: &str| DumpFormat::try_from(s))
                        .default_value(
                            "{fully_qualified_name} [{container}]: {permission} + {state}",
                        )
                        .long_help(HELP_DUMP_FORMAT.trim()),
                )
                .arg(
                    Arg::new("filter")
                        .long("filter")
                        .action(ArgAction::Append)
                        .long_help(HELP_DUMP_FILTER.trim()),
                )
                .arg(
                    Arg::new("dedup")
                        .long("dedup")
                        .num_args(0)
                        .action(ArgAction::SetTrue)
                        .long_help(HELP_DUMP_DEDUP.trim()),
                )
                .arg(Arg::new("out").long("out").default_value("-")),
        )
        .subcommand(
            Command::new("create-storage")
                .arg(
                    Arg::new("container")
                        .long("container")
                        .required(true)
                        .help("The target container for the generated storage file."),
                )
                .arg(
                    Arg::new("file")
                        .long("file")
                        .required(true)
                        .value_parser(|s: &str| StorageFileType::try_from(s))
                        .help("Type of storage file to create (pb, flatbuffer, test-mapping)."),
                )
                .arg(Arg::new("cache").long("cache").action(ArgAction::Append).required(true))
                .arg(Arg::new("out").long("out").required(true))
                .arg(
                    Arg::new("version")
                        .long("version")
                        .value_parser(|s: &str| s.parse::<u32>())
                        .help("Storage file format version."),
                ),
        )
}

fn get_required_arg<'a, T>(matches: &'a ArgMatches, arg_name: &str) -> Result<&'a T>
where
    T: Any + Clone + Send + Sync + 'static,
{
    matches
        .get_one::<T>(arg_name)
        .ok_or(anyhow!("internal error: required argument '{}' not found", arg_name))
}

fn get_zero_or_more_string_paths_from_arg(matches: &ArgMatches, arg_name: &str) -> Vec<String> {
    matches.get_many::<String>(arg_name).unwrap_or_default().cloned().collect()
}

// Process the raw arguments
// It will extract the arguments in response file if there is
pub fn process_raw_args<R: ResponseFileReader>(
    raw_args_iter: impl IntoIterator<Item = OsString>,
    reader: &R,
) -> Result<Vec<OsString>> {
    let mut processed_args: Vec<OsString> = Vec::new();
    let mut args_iter = raw_args_iter.into_iter();

    if let Some(app_arg) = args_iter.next() {
        processed_args.push(app_arg);
    }

    for arg in args_iter {
        let arg_str = arg.to_str().ok_or(anyhow!("Invalid argument: not a valid string"))?;

        if let Some(response_file_path) = arg_str.strip_prefix(RESPONSE_FILE_PREFIX) {
            ensure!(
                !response_file_path.is_empty(),
                "missing response file after {}",
                RESPONSE_FILE_PREFIX
            );
            let reader = reader
                .read_to_bufread(response_file_path)
                .with_context(|| format!("Failed to open response file: {response_file_path}"))?;
            for line_result in reader.lines() {
                let line = line_result.with_context(|| {
                    format!("Failed to read line from response file reader: {response_file_path}")
                })?;
                let trimmed_line = line.trim();
                if trimmed_line.is_empty() || trimmed_line.starts_with('#') {
                    continue;
                }
                for token in trimmed_line.split_whitespace() {
                    processed_args.push(OsString::from(token));
                }
            }
        } else {
            processed_args.push(arg);
        }
    }
    Ok(processed_args)
}

// Parses command line arguments, handling response files (@file).
// Returns a structured representation of the command or an error.
pub fn parse_args(
    processed_args: impl IntoIterator<Item = std::ffi::OsString>,
) -> Result<ParsedCommand> {
    let cli_app = build_cli();
    let matches = cli_app.get_matches_from(processed_args);

    match matches.subcommand() {
        Some(("create-cache", sub_matches)) => {
            let declarations = get_zero_or_more_string_paths_from_arg(sub_matches, "declarations");
            let values = get_zero_or_more_string_paths_from_arg(sub_matches, "values");
            let mainline_beta_namespace_config =
                match sub_matches.get_one::<String>("mainline-beta-namespace-config") {
                    Some(config) => {
                        if config.is_empty() {
                            None
                        } else {
                            Some(PathBuf::from(config))
                        }
                    }
                    None => None,
                };
            Ok(ParsedCommand::CreateCache {
                package: get_required_arg::<String>(sub_matches, "package")?.clone(),
                container: get_required_arg::<String>(sub_matches, "container")?.clone(),
                declarations,
                values,
                default_permission: *get_required_arg::<aconfig_protos::ProtoFlagPermission>(
                    sub_matches,
                    "default-permission",
                )?,
                allow_read_write: *get_required_arg::<bool>(sub_matches, "allow-read-write")?,
                cache_out_path: get_required_arg::<String>(sub_matches, "cache")?.clone(),
                mainline_beta_namespace_config,
                force_read_only: *get_required_arg::<bool>(sub_matches, "force-read-only")?,
            })
        }
        Some(("create-java-lib", sub_matches)) => Ok(ParsedCommand::CreateJavaLib {
            cache_path: get_required_arg::<String>(sub_matches, "cache")?.clone(),
            out_dir: PathBuf::from(get_required_arg::<String>(sub_matches, "out")?),
            mode: *get_required_arg::<CodegenMode>(sub_matches, "mode")?,
            single_exported_file: *get_required_arg::<bool>(sub_matches, "single-exported-file")?,
        }),
        Some(("create-cpp-lib", sub_matches)) => Ok(ParsedCommand::CreateCppLib {
            cache_path: get_required_arg::<String>(sub_matches, "cache")?.clone(),
            out_dir: PathBuf::from(get_required_arg::<String>(sub_matches, "out")?),
            mode: *get_required_arg::<CodegenMode>(sub_matches, "mode")?,
        }),
        Some(("create-rust-lib", sub_matches)) => Ok(ParsedCommand::CreateRustLib {
            cache_path: get_required_arg::<String>(sub_matches, "cache")?.clone(),
            out_dir: PathBuf::from(get_required_arg::<String>(sub_matches, "out")?),
            mode: *get_required_arg::<CodegenMode>(sub_matches, "mode")?,
        }),
        Some(("dump-cache", sub_matches)) => {
            let filters = sub_matches
                .get_many::<String>("filter")
                .unwrap_or_default()
                .cloned()
                .collect::<Vec<_>>();
            Ok(ParsedCommand::DumpCache {
                cache_paths: get_zero_or_more_string_paths_from_arg(sub_matches, "cache"),
                format: get_required_arg::<DumpFormat>(sub_matches, "format")?.clone(),
                filters,
                dedup: *get_required_arg::<bool>(sub_matches, "dedup")?,
                out_path: get_required_arg::<String>(sub_matches, "out")?.clone(),
            })
        }
        Some(("create-storage", sub_matches)) => {
            let version =
                sub_matches.get_one::<u32>("version").copied().unwrap_or(DEFAULT_FILE_VERSION);

            if version > MAX_SUPPORTED_FILE_VERSION {
                bail!(
                    "Invalid version selected ({}) for create-storage. Max supported: {}",
                    version,
                    MAX_SUPPORTED_FILE_VERSION
                );
            }

            Ok(ParsedCommand::CreateStorage {
                container: get_required_arg::<String>(sub_matches, "container")?.clone(),
                file_type: get_required_arg::<StorageFileType>(sub_matches, "file")?.clone(),
                cache_paths: get_zero_or_more_string_paths_from_arg(sub_matches, "cache"),
                out_path: get_required_arg::<String>(sub_matches, "out")?.clone(),
                version,
            })
        }
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;
    use std::io::{BufRead, BufReader, Cursor};
    use std::path::PathBuf;

    #[derive(Default)]
    struct MockFileReader {
        files: HashMap<String, String>,
    }

    impl MockFileReader {
        fn add_file(&mut self, path: &str, content: &str) {
            self.files.insert(path.to_string(), content.to_string());
        }
    }

    impl ResponseFileReader for MockFileReader {
        fn read_to_bufread(&self, path_str: &str) -> Result<Box<dyn BufRead>> {
            match self.files.get(path_str) {
                Some(content) => {
                    let data = content.clone().into_bytes();
                    let cursor = Cursor::new(data);
                    let buf_reader = BufReader::new(cursor);
                    Ok(Box::new(buf_reader))
                }
                None => Err(anyhow!("Mock file not found: {}", path_str)),
            }
        }
    }

    fn create_os_command(command: &str) -> Vec<OsString> {
        command.split_whitespace().map(OsString::from).collect()
    }

    #[test]
    fn test_parse_create_cache() -> Result<()> {
        let command_string = "aconfig create-cache \
             --package com.test.cache \
             --container vendor \
             --declarations test.aconfig \
             --values flag.val \
             --cache /output/cache.pb \
             --default-permission READ_WRITE \
             --allow-read-write true \
             --mainline-beta-namespace-config /path/to/some/file.json \
             --force-read-only false";
        let input_args = create_os_command(command_string);
        let parsed = parse_args(input_args)?;

        assert!(matches!(parsed, ParsedCommand::CreateCache { .. }));
        if let ParsedCommand::CreateCache {
            package,
            container,
            declarations,
            values,
            default_permission,
            allow_read_write,
            cache_out_path,
            mainline_beta_namespace_config,
            force_read_only,
        } = parsed
        {
            assert_eq!(package, "com.test.cache");
            assert_eq!(container, "vendor".to_string());
            assert_eq!(declarations.len(), 1);
            assert_eq!(values.len(), 1);
            assert_eq!(declarations[0], "test.aconfig");
            assert_eq!(values[0], "flag.val");
            assert_eq!(default_permission, aconfig_protos::ProtoFlagPermission::READ_WRITE);
            assert!(allow_read_write);
            assert_eq!(cache_out_path, "/output/cache.pb");
            assert_eq!(
                mainline_beta_namespace_config,
                Some(PathBuf::from("/path/to/some/file.json"))
            );
            assert!(!force_read_only);
        }
        Ok(())
    }

    #[test]
    fn test_parse_create_java_lib() -> Result<()> {
        let command_string = "aconfig create-java-lib \
             --cache cache.pb \
             --out /java/output \
             --mode test \
             --single-exported-file true";
        let input_args = create_os_command(command_string);
        let parsed = parse_args(input_args)?;

        assert!(matches!(parsed, ParsedCommand::CreateJavaLib { .. }));
        if let ParsedCommand::CreateJavaLib { cache_path, out_dir, mode, single_exported_file } =
            parsed
        {
            assert_eq!(cache_path, "cache.pb");
            assert_eq!(out_dir, PathBuf::from("/java/output"));
            assert_eq!(mode, CodegenMode::Test);
            assert!(single_exported_file);
        }
        Ok(())
    }

    #[test]
    fn test_parse_create_cpp_lib() -> Result<()> {
        let command_string = "aconfig create-cpp-lib \
             --cache cache.pb \
             --out /cpp/output \
             --mode test";
        let input_args = create_os_command(command_string);
        let parsed = parse_args(input_args)?;

        assert!(matches!(parsed, ParsedCommand::CreateCppLib { .. }));
        if let ParsedCommand::CreateCppLib { cache_path, out_dir, mode } = parsed {
            assert_eq!(cache_path, "cache.pb");
            assert_eq!(out_dir, PathBuf::from("/cpp/output"));
            assert_eq!(mode, CodegenMode::Test);
        }
        Ok(())
    }

    #[test]
    fn test_parse_dump_cache() -> Result<()> {
        let command_string = "aconfig dump-cache \
             --cache cache1.pb \
             --cache cache2.pb \
             --format textproto \
             --filter package:com.foo \
             --filter state:ENABLED+name:bar \
             --dedup \
             --out /tmp/dump.txt";

        let input_args = create_os_command(command_string);
        let parsed = parse_args(input_args)?;

        assert!(matches!(parsed, ParsedCommand::DumpCache { .. }));
        if let ParsedCommand::DumpCache { cache_paths, format, filters, dedup, out_path } = parsed {
            assert_eq!(cache_paths.len(), 2);
            assert!(cache_paths.iter().any(|c| c == "cache1.pb"));
            assert!(cache_paths.iter().any(|c| c == "cache2.pb"));
            assert_eq!(format, DumpFormat::Textproto);
            assert_eq!(filters.len(), 2);
            assert_eq!(filters[0], "package:com.foo");
            assert_eq!(filters[1], "state:ENABLED+name:bar");
            assert!(dedup);
            assert_eq!(out_path, "/tmp/dump.txt");
        }
        Ok(())
    }

    #[test]
    fn test_parse_create_storage() -> Result<()> {
        let version = DEFAULT_FILE_VERSION + 1;

        let command_string = format!(
            "aconfig create-storage \
             --container system \
             --file package_map \
             --cache cache1.pb \
             --cache cache2.pb \
             --out /storage/system.package.map \
             --version {version}"
        );
        let input_args = create_os_command(&command_string);
        let parsed = parse_args(input_args)?;

        assert!(matches!(parsed, ParsedCommand::CreateStorage { .. }));
        if let ParsedCommand::CreateStorage {
            container,
            file_type,
            cache_paths,
            out_path,
            version: parsed_version,
        } = parsed
        {
            assert_eq!(container, "system");
            assert_eq!(file_type, StorageFileType::PackageMap);
            assert_eq!(cache_paths.len(), 2);
            assert!(cache_paths.iter().any(|c| c == "cache1.pb"));
            assert!(cache_paths.iter().any(|c| c == "cache2.pb"));
            assert_eq!(out_path, "/storage/system.package.map");
            assert_eq!(parsed_version, version);
        }
        Ok(())
    }

    #[test]
    fn test_parse_create_rust_lib() -> Result<()> {
        let command_string = "aconfig create-rust-lib \
             --cache cache.pb \
             --out /rust/output \
             --mode test"
            .to_string();
        let input_args = create_os_command(&command_string);
        let parsed = parse_args(input_args)?;

        assert!(matches!(parsed, ParsedCommand::CreateRustLib { .. }));
        if let ParsedCommand::CreateRustLib { cache_path, out_dir, mode } = parsed {
            assert_eq!(cache_path, "cache.pb");
            assert_eq!(out_dir, PathBuf::from("/rust/output"));
            assert_eq!(mode, CodegenMode::Test);
        }
        Ok(())
    }

    #[test]
    fn test_process_args_with_response_file() -> Result<()> {
        let mut reader = MockFileReader::default();
        reader.add_file("args.txt", "--option1 value1\n#comment\n--flag value2");

        let raw_args = create_os_command("aconfig dump @args.txt --other value3");
        let expected_args: Vec<OsString> = vec![
            "aconfig".into(),
            "dump".into(),
            "--option1".into(),
            "value1".into(),
            "--flag".into(),
            "value2".into(),
            "--other".into(),
            "value3".into(),
        ];

        let processed = process_raw_args(raw_args, &reader)?;

        assert_eq!(processed, expected_args);
        Ok(())
    }

    #[test]
    fn test_response_file_expansion() -> Result<()> {
        let file_content = r#"
        --package
        com.via.respfile
        # This is a comment

        --container
        vendor
        --cache
        cache.pb
        "#;
        let extra_command = "--declarations test.aconfig \
             --values flag.val";
        let mut reader = MockFileReader::default();
        reader.add_file("response", file_content);

        let mut input_args: Vec<OsString> =
            vec!["aconfig".into(), "create-cache".into(), "@response".into()];
        input_args.append(&mut create_os_command(extra_command));

        let processed = process_raw_args(input_args, &reader)?;
        let parsed = parse_args(processed)?;

        assert!(matches!(parsed, ParsedCommand::CreateCache { .. }));
        if let ParsedCommand::CreateCache {
            package,
            container,
            declarations,
            values,
            cache_out_path,
            default_permission,
            allow_read_write,
            mainline_beta_namespace_config,
            force_read_only,
        } = parsed
        {
            assert_eq!(package, "com.via.respfile");
            assert_eq!(container, "vendor");
            assert_eq!(declarations.len(), 1);
            assert_eq!(values.len(), 1);
            assert_eq!(declarations[0], "test.aconfig");
            assert_eq!(values[0], "flag.val");
            assert_eq!(default_permission, aconfig_protos::ProtoFlagPermission::READ_WRITE);
            assert!(allow_read_write);
            assert_eq!(cache_out_path, "cache.pb");
            assert_eq!(mainline_beta_namespace_config, None);
            assert!(!force_read_only);
        }

        Ok(())
    }

    #[test]
    fn test_response_file_expansion_empty_file() -> Result<()> {
        let file_content = r#""#;
        let extra_command = "--package \
            com.via.respfile \
            --container \
            vendor \
            --cache \
            cache.pb \
            --declarations  test.aconfig \
            --values flag.val";
        let mut reader = MockFileReader::default();
        reader.add_file("response", file_content);
        let mut input_args: Vec<OsString> =
            vec!["aconfig".into(), "create-cache".into(), "@response".into()];
        input_args.append(&mut create_os_command(extra_command));
        let processed = process_raw_args(input_args, &reader)?;
        let parsed = parse_args(processed)?;

        assert!(matches!(parsed, ParsedCommand::CreateCache { .. }));
        if let ParsedCommand::CreateCache {
            package,
            container,
            declarations,
            values,
            cache_out_path,
            default_permission,
            allow_read_write,
            mainline_beta_namespace_config,
            force_read_only,
        } = parsed
        {
            assert_eq!(package, "com.via.respfile");
            assert_eq!(container, "vendor");
            assert_eq!(declarations.len(), 1);
            assert_eq!(values.len(), 1);
            assert_eq!(declarations[0], "test.aconfig");
            assert_eq!(values[0], "flag.val");
            assert_eq!(default_permission, aconfig_protos::ProtoFlagPermission::READ_WRITE);
            assert!(allow_read_write);
            assert_eq!(cache_out_path, "cache.pb");
            assert_eq!(mainline_beta_namespace_config, None);
            assert!(!force_read_only);
        }

        Ok(())
    }
}
