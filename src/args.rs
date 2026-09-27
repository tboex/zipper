use std::{ffi::OsString, path::PathBuf};

pub const HELP: &str = "zipper — gzip compression and decompression

Usage:
  zipper compress [INPUT] [-o OUTPUT] [--level 0..9] [-v]
  zipper decompress [INPUT] [-o OUTPUT] [--max-output BYTES] [-v]
  zipper test [INPUT] [--max-output BYTES]

INPUT and OUTPUT default to '-' (stdin/stdout).
File output never overwrites an existing path. Source files are kept.
Compression level defaults to 6. Decoding is limited to 1073741824 bytes (1 GiB).
Use --max-output to choose a different nonnegative byte limit.
Use -- before an input filename that begins with a dash.

  -o, --output FILE       Output destination
  -v, --verbose           Print byte counts to stderr
  -h, --help              Print this help
  -V, --version           Print version";

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Command {
    Compress,
    Decompress,
    Test,
}

pub struct Options {
    pub command: Command,
    pub input: PathBuf,
    pub output: PathBuf,
    pub level: u32,
    pub max_output: u64,
    pub verbose: bool,
}

pub enum Action {
    Help,
    Version,
    Run(Options),
}

pub fn parse(args: impl Iterator<Item = OsString>) -> Result<Action, String> {
    let mut args = args.peekable();
    let Some(command) = args.next() else {
        return Ok(Action::Help);
    };
    let command = match command.to_str() {
        Some("-h" | "--help") => return Ok(Action::Help),
        Some("-V" | "--version") => return Ok(Action::Version),
        Some("compress") => Command::Compress,
        Some("decompress") => Command::Decompress,
        Some("test") => Command::Test,
        _ => return Err("expected compress, decompress, or test".into()),
    };
    let mut input = None;
    let mut output = None;
    let mut level = None;
    let mut max_output = None;
    let mut verbose = false;
    let mut positional = false;
    while let Some(arg) = args.next() {
        if !positional {
            match arg.to_str() {
                Some("--") => {
                    positional = true;
                    continue;
                }
                Some("-h" | "--help") => return Ok(Action::Help),
                Some("-v" | "--verbose") => {
                    verbose = true;
                    continue;
                }
                Some("-o" | "--output") => {
                    if output.is_some() {
                        return Err("output specified twice".into());
                    }
                    output = Some(PathBuf::from(args.next().ok_or("missing output path")?));
                    continue;
                }
                Some("--level") => {
                    if level.is_some() {
                        return Err("level specified twice".into());
                    }
                    level = Some(number(args.next(), "level")?);
                    continue;
                }
                Some("--max-output") => {
                    if max_output.is_some() {
                        return Err("max-output specified twice".into());
                    }
                    max_output = Some(number(args.next(), "max-output")?);
                    continue;
                }
                _ if arg.as_encoded_bytes().starts_with(b"-") && arg != "-" => {
                    return Err(format!("unknown option: {}", arg.to_string_lossy()))
                }
                _ => {}
            }
        }
        if input.replace(PathBuf::from(arg)).is_some() {
            return Err("only one input is supported".into());
        }
    }
    if level.is_some() && command != Command::Compress {
        return Err("--level is only valid for compress".into());
    }
    if max_output.is_some() && command == Command::Compress {
        return Err("--max-output is only valid for decompress or test".into());
    }
    if output.is_some() && command == Command::Test {
        return Err("test does not accept an output path".into());
    }
    let level = level.unwrap_or(6);
    if level > 9 {
        return Err("level must be 0–9".into());
    }
    Ok(Action::Run(Options {
        command,
        input: input.unwrap_or_else(|| "-".into()),
        output: output.unwrap_or_else(|| "-".into()),
        level: level as u32,
        max_output: max_output.unwrap_or(1024 * 1024 * 1024),
        verbose,
    }))
}

fn number(value: Option<OsString>, name: &str) -> Result<u64, String> {
    value
        .and_then(|v| v.to_str().and_then(|s| s.parse().ok()))
        .ok_or_else(|| format!("{name} requires a nonnegative integer"))
}
