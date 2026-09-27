use std::io::{self, Read, Write};

fn encode(input: &[u8], level: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let stats = zipper::compress(input, &mut out, level).unwrap();
    assert_eq!(stats.input_bytes, input.len() as u64);
    assert_eq!(stats.output_bytes, out.len() as u64);
    out
}

#[test]
fn round_trips_empty_text_and_binary_at_every_level() {
    let binary: Vec<_> = (0..100_000)
        .map(|i| ((i * 37 + i / 13) % 256) as u8)
        .collect();
    for level in 0..=9 {
        for input in [b"".as_slice(), "hello 🦀\n".as_bytes(), binary.as_slice()] {
            let encoded = encode(input, level);
            let mut decoded = Vec::new();
            let stats =
                zipper::decompress(encoded.as_slice(), &mut decoded, Some(input.len() as u64))
                    .unwrap();
            assert_eq!(decoded, input);
            assert_eq!(stats.input_bytes, encoded.len() as u64);
            assert_eq!(stats.output_bytes, input.len() as u64);
        }
    }
}

#[test]
fn deterministic_header_and_output() {
    let a = encode(b"same input", 6);
    assert_eq!(a, encode(b"same input", 6));
    assert_eq!(&a[..3], &[0x1f, 0x8b, 8]);
    assert_eq!(&a[4..8], &[0, 0, 0, 0]);
    assert_eq!(a[9], 255);
}

#[test]
fn concatenated_members_and_combined_limit() {
    let mut encoded = encode(b"hello", 6);
    encoded.extend(encode(b"", 6));
    encoded.extend(encode(b" world", 6));
    let mut out = Vec::new();
    zipper::decompress(encoded.as_slice(), &mut out, Some(11)).unwrap();
    assert_eq!(out, b"hello world");
    assert!(zipper::verify(encoded.as_slice(), Some(10)).is_err());
}

#[test]
fn rejects_corruption_truncation_and_trailing_junk() {
    let encoded = encode(b"checksum me", 6);
    for len in 0..encoded.len() {
        assert!(
            zipper::verify(&encoded[..len], None).is_err(),
            "accepted truncation at {len}"
        );
    }
    for offset in [0, encoded.len() - 8, encoded.len() - 4] {
        let mut damaged = encoded.clone();
        damaged[offset] ^= 0xff;
        assert!(zipper::verify(damaged.as_slice(), None).is_err());
    }
    let mut junk = encoded;
    junk.extend_from_slice(b"junk");
    assert!(zipper::verify(junk.as_slice(), None).is_err());
    assert!(zipper::verify(&b"not gzip"[..], None).is_err());
}

#[test]
fn output_limit_never_writes_past_budget() {
    let encoded = encode(&vec![0; 200_000], 9);
    let mut out = Vec::new();
    assert!(zipper::decompress(encoded.as_slice(), &mut out, Some(40_000)).is_err());
    assert!(out.len() <= 40_000);
    assert!(zipper::verify(encoded.as_slice(), Some(0)).is_err());
    assert_eq!(
        zipper::verify(encode(b"", 6).as_slice(), Some(0))
            .unwrap()
            .output_bytes,
        0
    );
}

#[test]
fn decodes_optional_filename_and_comment() {
    let mut encoder = flate2::GzBuilder::new()
        .filename("original.txt")
        .comment("fixture")
        .mtime(12345)
        .write(Vec::new(), flate2::Compression::default());
    encoder.write_all(b"metadata").unwrap();
    let encoded = encoder.finish().unwrap();
    let mut out = Vec::new();
    zipper::decompress(encoded.as_slice(), &mut out, None).unwrap();
    assert_eq!(out, b"metadata");
}

struct TinyReader<'a> {
    bytes: &'a [u8],
    interrupted: bool,
}
impl Read for TinyReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if !self.interrupted {
            self.interrupted = true;
            return Err(io::ErrorKind::Interrupted.into());
        }
        let n = buffer.len().min(3).min(self.bytes.len());
        buffer[..n].copy_from_slice(&self.bytes[..n]);
        self.bytes = &self.bytes[n..];
        Ok(n)
    }
}

#[test]
fn handles_short_reads_and_interrupted_io() {
    let mut encoded = Vec::new();
    zipper::compress(
        TinyReader {
            bytes: b"tiny reads",
            interrupted: false,
        },
        &mut encoded,
        6,
    )
    .unwrap();
    let mut out = Vec::new();
    zipper::decompress(
        TinyReader {
            bytes: &encoded,
            interrupted: false,
        },
        &mut out,
        None,
    )
    .unwrap();
    assert_eq!(out, b"tiny reads");
}

struct BrokenWriter;
impl Write for BrokenWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct FlushFailure;
impl Write for FlushFailure {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("flush failed"))
    }
}

#[test]
fn propagates_write_flush_and_option_errors() {
    assert!(zipper::compress(&b"data"[..], BrokenWriter, 6).is_err());
    assert!(zipper::compress(&b"data"[..], FlushFailure, 6).is_err());
    assert!(zipper::compress(&b"data"[..], Vec::new(), 10).is_err());
    let encoded = encode(b"data", 6);
    assert!(zipper::decompress(encoded.as_slice(), BrokenWriter, None).is_err());
    assert!(zipper::decompress(encoded.as_slice(), FlushFailure, None).is_err());
}
