mod args;

use args::{Action, Options};
use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter, IsTerminal, Read, Write},
    path::Path,
    process::ExitCode,
};

fn main() -> ExitCode {
    match args::parse(std::env::args_os().skip(1)) {
        Ok(Action::Help) => {
            println!("{}", args::HELP);
            ExitCode::SUCCESS
        }
        Ok(Action::Version) => {
            println!("zipper {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Action::Run(options)) => match run(options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("zipper: {e}");
                ExitCode::from(1)
            }
        },
        Err(e) => {
            eprintln!("zipper: {e}\nTry 'zipper --help'.");
            ExitCode::from(2)
        }
    }
}

fn run(options: Options) -> io::Result<()> {
    let input: Box<dyn Read> = if options.input == Path::new("-") {
        Box::new(io::stdin().lock())
    } else {
        let file = File::open(&options.input)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "input must be a regular file or stdin",
            ));
        }
        Box::new(BufReader::new(file))
    };
    let stats = if options.command == args::Command::Test {
        zipper::verify(input, Some(options.max_output))?
    } else if options.output == Path::new("-") {
        if options.command == args::Command::Compress && io::stdout().is_terminal() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "refusing to write compressed bytes to a terminal; use -o FILE or pipe stdout",
            ));
        }
        let stdout = io::stdout();
        transform(&options, input, BufWriter::new(stdout.lock()))?
    } else {
        // Stage in the destination directory. persist_noclobber also closes the
        // race where another process creates the destination after this check.
        match fs::symlink_metadata(&options.output) {
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "output already exists",
                ))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let parent = options
            .output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut staged = tempfile::NamedTempFile::new_in(parent)?;
        let stats = transform(&options, input, BufWriter::new(staged.as_file_mut()))?;
        staged.as_file().sync_all()?;
        staged
            .persist_noclobber(&options.output)
            .map_err(|e| e.error)?;
        stats
    };
    if options.verbose || options.command == args::Command::Test {
        eprintln!(
            "ok: {} input bytes, {} output bytes",
            stats.input_bytes, stats.output_bytes
        );
    }
    Ok(())
}

fn transform<R: Read, W: Write>(
    options: &Options,
    input: R,
    output: W,
) -> io::Result<zipper::Stats> {
    match options.command {
        args::Command::Compress => zipper::compress(input, output, options.level),
        args::Command::Decompress => zipper::decompress(input, output, Some(options.max_output)),
        args::Command::Test => unreachable!("test handled separately"),
    }
}

// TODO: Add optional progress reporting for large files, on stderr only.
