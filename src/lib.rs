//! Streaming gzip helpers backed by flate2's Rust implementation.
//!
//! ```
//! let mut compressed = Vec::new();
//! zipper::compress(&b"hello gzip"[..], &mut compressed, 6)?;
//! let mut restored = Vec::new();
//! zipper::decompress(compressed.as_slice(), &mut restored, Some(1024))?;
//! assert_eq!(restored, b"hello gzip");
//! # Ok::<(), std::io::Error>(())
//! ```

use flate2::{read::MultiGzDecoder, Compression, GzBuilder};
use std::io::{self, Read, Write};

/// Successful stream byte counts, including gzip headers and trailers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    pub input_bytes: u64,
    pub output_bytes: u64,
}

/// Compresses into one gzip member with a level from 0 (stored) to 9 (best).
/// The header omits filenames and timestamps. The writer is flushed on success.
/// An error may leave partial output in the supplied writer.
pub fn compress<R: Read, W: Write>(mut input: R, output: W, level: u32) -> io::Result<Stats> {
    if level > 9 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "compression level must be 0–9",
        ));
    }
    let counter = CountWriter {
        inner: output,
        bytes: 0,
    };
    let mut encoder = GzBuilder::new()
        .mtime(0)
        .operating_system(255)
        .write(counter, Compression::new(level));
    let input_bytes = io::copy(&mut input, &mut encoder)?;
    let mut counter = encoder.finish()?;
    counter.flush()?;
    Ok(Stats {
        input_bytes,
        output_bytes: counter.bytes,
    })
}

/// Decompresses all concatenated gzip members and verifies their trailers.
/// `max_output` bounds the total decoded bytes across all members; `None` is unlimited.
/// Invalid/truncated input or trailing non-gzip bytes return an error.
/// Output already written cannot be retracted on errors, including checksum failures.
pub fn decompress<R: Read, W: Write>(
    input: R,
    mut output: W,
    max_output: Option<u64>,
) -> io::Result<Stats> {
    let mut counter = CountReader {
        inner: input,
        bytes: 0,
    };
    let mut decoder = MultiGzDecoder::new(&mut counter);
    let mut buffer = [0; 32 * 1024];
    let mut output_bytes = 0u64;
    loop {
        let read = match decoder.read(&mut buffer) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if read == 0 {
            break;
        }
        let next = output_bytes
            .checked_add(read as u64)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "output size overflow"))?;
        if max_output.is_some_and(|limit| next > limit) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "decompressed output exceeds the byte limit",
            ));
        }
        output.write_all(&buffer[..read])?;
        output_bytes = next;
    }
    output.flush()?;
    Ok(Stats {
        input_bytes: counter.bytes,
        output_bytes,
    })
}

/// Validates a gzip stream without retaining its decoded contents.
pub fn verify<R: Read>(input: R, max_output: Option<u64>) -> io::Result<Stats> {
    decompress(input, io::sink(), max_output)
}

struct CountReader<R> {
    inner: R,
    bytes: u64,
}
impl<R: Read> Read for CountReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.bytes += n as u64;
        Ok(n)
    }
}
struct CountWriter<W> {
    inner: W,
    bytes: u64,
}
impl<W: Write> Write for CountWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.bytes += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
