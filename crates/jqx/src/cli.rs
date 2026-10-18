//! Command-line argument parsing and configuration for `jqx`.
//!
//! Hand-written parser using `std::env::args_os` with zero external dependencies.

use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Default,
    Raw,
    Compact,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliConfig {
    pub format: OutputFormat,
    pub explain: bool,
    pub exit_status: bool,
    pub query: String,
    pub file: Option<PathBuf>,
}

impl CliConfig {
    pub fn parse() -> Result<Self, String> {
        Self::parse_from(std::env::args_os())
    }

    pub fn parse_from<I, T>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString>,
    {
        let mut format = OutputFormat::Default;
        let mut explain = false;
        let mut exit_status = false;
        let mut positional = Vec::new();

        let iter = args.into_iter().map(Into::into).skip(1);
        for arg in iter {
            let str_val = arg.to_string_lossy();
            match str_val.as_ref() {
                "-r" | "--raw" => format = OutputFormat::Raw,
                "-c" | "--compact" => format = OutputFormat::Compact,
                "--explain" => explain = true,
                "-e" | "--exit-status" => exit_status = true,
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                "-V" | "--version" => {
                    println!("jqx {}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                other if other.starts_with('-') && other != "-" => {
                    return Err(format!("unknown option `{other}`"));
                }
                _ => positional.push(arg),
            }
        }

        if positional.is_empty() {
            return Err("missing query argument; usage: jqx [OPTIONS] <QUERY> [FILE...]".to_string());
        }

        let query = positional[0].to_string_lossy().to_string();
        let file = if positional.len() > 1 && positional[1] != "-" {
            Some(PathBuf::from(&positional[1]))
        } else {
            None
        };

        Ok(Self {
            format,
            explain,
            exit_status,
            query,
            file,
        })
    }
}

pub fn print_help() {
    println!(
        r"jqx - JSON Query Executor

USAGE:
    jqx [OPTIONS] <QUERY> [FILE...]

ARGS:
    <QUERY>    JavaScript-subset query expression (e.g. 'users.filter(u => u.active)')
    [FILE...]  JSON input files (defaults to stdin)

OPTIONS:
    -r, --raw          Print top-level strings without quotes
    -c, --compact      Output compact JSON (one line per output)
    -e, --exit-status  Exit 1 if result is null or absent
    --explain          Print execution plan, memory tier, and early exit capabilities
    -h, --help         Print help information
    -V, --version      Print version information
"
    );
}
