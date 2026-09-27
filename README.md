# zipper

Gzip compression and decompression in Rust, as a command-line tool and a small
streaming library. Handles files, pipes, and concatenated gzip streams.

The compression engine is [flate2](https://docs.rs/flate2/) with its Rust backend.
Zipper adds the CLI, byte counts, output limits, and file handling. It doesn't
implement DEFLATE from scratch, and `.gz` files aren't ZIP archives.

## Try it

```sh
cargo build --release

./target/release/zipper compress notes.txt -o notes.txt.gz
./target/release/zipper decompress notes.txt.gz -o restored.txt
./target/release/zipper test notes.txt.gz
```

Or install locally with `cargo install --path .` to put `zipper` on your PATH.

Input and output default to stdin and stdout. Use `-` explicitly if you prefer:

```sh
printf 'hello gzip\n' | zipper compress | zipper decompress
zipper compress large.log --level 9 -o large.log.gz -v
zipper decompress large.log.gz --max-output 2147483648 -o restored.log
```

Compression levels range from 0 (no compression) to 9 (best); the default is 6.
`-v` prints byte counts to stderr, keeping stdout usable in a pipeline. `test`
checks the entire stream without saving the decoded bytes and reports counts
on stderr. Use `--` before input filenames that begin with a dash.

## File and stream behavior

- Input files are always kept. Existing output paths are never overwritten,
  including symlinks and the input file itself.
- File output is staged in the destination directory and published only after
  compression or validation finishes. Normal failures clean up the temporary file.
- Stdout and library writers can receive partial output before an error is found.
  A gzip checksum is checked at the end, so don't treat partial output as verified.
- Decompression reads every concatenated member, checking checksums and decoded
  sizes. Truncated input and trailing non-gzip bytes are errors.
- The CLI limits decoded output to 1 GiB by default. `--max-output BYTES` changes
  that limit across all members; zero only allows empty output.
- Generated headers omit filenames and use a zero timestamp. Output is repeatable
  for the same input, level, and backend version. Original metadata isn't restored.

The CLI refuses compressed output to an interactive terminal. Errors exit with
code 1; argument errors use code 2. File staging isn't a power-loss durability
guarantee, and a forced process kill may leave a temporary file.

## Library

The crate isn't published. Add it as a local dependency:

```toml
[dependencies]
zipper = { path = "../zipper" }
```

```rust
use std::io;

fn main() -> io::Result<()> {
    let mut encoded = Vec::new();
    let stats = zipper::compress(&b"hello gzip"[..], &mut encoded, 6)?;

    let mut decoded = Vec::new();
    zipper::decompress(encoded.as_slice(), &mut decoded, Some(1024))?;

    assert_eq!(decoded, b"hello gzip");
    println!("{} -> {} bytes", stats.input_bytes, stats.output_bytes);
    Ok(())
}
```

`compress`, `decompress`, and `verify` work with `std::io::Read`/`Write` streams.
Pass `None` as the library's output limit for unlimited decoding. Processing uses
bounded buffers rather than loading whole files; callers choosing a `Vec` writer
still retain the full output. These APIs are synchronous.

## Development

```sh
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

Tests cover every compression level, binary/empty input, multiple members,
damaged trailers, truncation, limits, short reads, write failures, file protection,
and pipelines. Unix tests also use the system `gzip` command to check compatibility
in both directions. CI is configured for Linux, macOS, and Windows.

## TODO

- Add optional progress reporting for large files.
- Benchmark compression levels on text and binary inputs.
- Add a batch mode for compressing several independent files.

## License

[MIT](LICENSE)
