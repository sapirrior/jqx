//! Main entrypoint for the `jqx` CLI binary.
//!
//! Orchestrates CLI arguments, input reading, query compilation, execution, and exit codes.

#![forbid(unsafe_code)]

mod cli;

use cli::CliConfig;
use jqx_engine::Query;
use jqx_eval::Budget;
use jqx_json::Value;
use std::fs::File;
use std::io::{self, BufReader, Read};

// Exit codes per jqx.md §5.4
const EXIT_OK: i32 = 0;
const EXIT_EMPTY: i32 = 1;
const EXIT_USAGE: i32 = 2;
const EXIT_COMPILE: i32 = 3;
const EXIT_INPUT: i32 = 4;
const EXIT_RUNTIME: i32 = 5;
const EXIT_LIMIT: i32 = 6;

fn main() {
    let code = run();
    std::process::exit(code);
}

fn run() -> i32 {
    let config = match CliConfig::parse() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error[U0001]: {e}");
            return EXIT_USAGE;
        }
    };

    let query = match Query::compile(&config.query) {
        Ok(q) => q,
        Err(diag) => {
            eprint!("{}", diag.render(&config.query));
            return EXIT_COMPILE;
        }
    };

    if config.explain {
        print!("{}", query.explain());
        return EXIT_OK;
    }

    let input_reader: Box<dyn Read> = match &config.file {
        Some(path) => match File::open(path) {
            Ok(f) => Box::new(BufReader::new(f)),
            Err(e) => {
                eprintln!("error[I0001]: failed to open file `{}`: {e}", path.display());
                return EXIT_INPUT;
            }
        },
        None => Box::new(BufReader::new(io::stdin())),
    };

    let budget = Budget::default();
    match query.run_reader(input_reader, budget) {
        Ok(val) => {
            if val.is_absent() {
                if config.exit_status {
                    return EXIT_EMPTY;
                }
                return EXIT_OK;
            }

            if config.exit_status && val.is_null() {
                return EXIT_EMPTY;
            }

            emit_value(&val, config.format);
            EXIT_OK
        }
        Err(diag) => {
            eprint!("{}", diag.render(&config.query));
            match diag.code {
                "R0001" | "R0002" => EXIT_LIMIT,
                "J0001" => EXIT_INPUT,
                _ => EXIT_RUNTIME,
            }
        }
    }
}

fn emit_value(val: &Value<'_>, format: cli::OutputFormat) {
    if format == cli::OutputFormat::Raw {
        if let Some(s) = val.as_str() {
            println!("{s}");
            return;
        }
    }
    println!("{val}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing_invariant() {
        // High-risk invariant: verify flag extraction and positional arguments
        let args = vec!["jqx", "-r", "users.map(u => u.name)", "data.json"];
        let config = CliConfig::parse_from(args).unwrap();

        assert_eq!(config.format, cli::OutputFormat::Raw);
        assert_eq!(config.query, "users.map(u => u.name)");
        assert_eq!(config.file.unwrap().to_str().unwrap(), "data.json");
    }
}
