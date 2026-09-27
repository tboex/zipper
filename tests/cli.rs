use std::{
    fs,
    io::Write,
    process::{Command, Output, Stdio},
};

fn zipper() -> Command {
    Command::new(env!("CARGO_BIN_EXE_zipper"))
}

fn piped(mut command: Command, data: &[u8]) -> Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let owned = data.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&owned));
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap().unwrap();
    output
}

#[test]
fn stdin_stdout_round_trip() {
    let mut compress = zipper();
    compress.args(["compress", "--level", "9", "-v"]);
    let compressed = piped(compress, b"pipe contents\n");
    assert!(compressed.status.success());
    assert!(String::from_utf8_lossy(&compressed.stderr).contains("input bytes"));
    let mut decompress = zipper();
    decompress.arg("decompress");
    let decoded = piped(decompress, &compressed.stdout);
    assert!(decoded.status.success());
    assert_eq!(decoded.stdout, b"pipe contents\n");
    assert!(decoded.stderr.is_empty());
}

#[test]
fn file_round_trip_keeps_sources_and_refuses_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.txt");
    let compressed = dir.path().join("input.gz");
    let restored = dir.path().join("restored.txt");
    fs::write(&input, b"keep this source").unwrap();
    assert!(zipper()
        .arg("compress")
        .arg(&input)
        .arg("-o")
        .arg(&compressed)
        .output()
        .unwrap()
        .status
        .success());
    assert!(input.exists());
    assert!(zipper()
        .arg("decompress")
        .arg(&compressed)
        .arg("-o")
        .arg(&restored)
        .output()
        .unwrap()
        .status
        .success());
    assert_eq!(fs::read(&input).unwrap(), fs::read(&restored).unwrap());
    let prior = fs::read(&compressed).unwrap();
    assert!(!zipper()
        .arg("compress")
        .arg(&input)
        .arg("-o")
        .arg(&compressed)
        .output()
        .unwrap()
        .status
        .success());
    assert_eq!(fs::read(&compressed).unwrap(), prior);
    assert!(!zipper()
        .arg("compress")
        .arg(&input)
        .arg("-o")
        .arg(&input)
        .output()
        .unwrap()
        .status
        .success());
    assert_eq!(fs::read(&input).unwrap(), b"keep this source");
}

#[test]
fn failed_decode_does_not_leave_output_or_temporary_files() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.gz");
    let output = dir.path().join("output.txt");
    let mut gzip = Vec::new();
    zipper::compress(&b"bad checksum"[..], &mut gzip, 6).unwrap();
    let crc_offset = gzip.len() - 8;
    gzip[crc_offset] ^= 1;
    fs::write(&bad, gzip).unwrap();
    let result = zipper()
        .arg("decompress")
        .arg(&bad)
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(!output.exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn test_command_and_limit_failure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.gz");
    let mut encoded = Vec::new();
    zipper::compress(&b"12345"[..], &mut encoded, 6).unwrap();
    fs::write(&path, encoded).unwrap();
    let valid = zipper()
        .arg("test")
        .arg(&path)
        .args(["--max-output", "5"])
        .output()
        .unwrap();
    assert!(valid.status.success());
    assert!(valid.stdout.is_empty());
    let output = dir.path().join("output");
    let invalid = zipper()
        .arg("decompress")
        .arg(&path)
        .args(["--max-output", "4", "-o"])
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(invalid.status.code(), Some(1));
    assert!(!output.exists());
}

#[test]
fn validates_cli_options() {
    for args in [
        vec!["compress", "--level", "10"],
        vec!["compress", "--level", "-1"],
        vec!["test", "-o", "file"],
        vec!["decompress", "--level", "5"],
        vec!["compress", "--max-output", "100"],
        vec!["compress", "a", "b"],
        vec!["compress", "-o"],
        vec!["decompress", "--max-output", "oops"],
        vec!["unknown"],
    ] {
        assert_eq!(
            zipper().args(&args).output().unwrap().status.code(),
            Some(2),
            "{args:?}"
        );
    }
    assert!(zipper().arg("--help").output().unwrap().status.success());
    assert!(zipper().arg("--version").output().unwrap().status.success());
}

#[test]
fn dash_prefixed_file_after_separator() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("-file"), b"dash").unwrap();
    let output = zipper()
        .current_dir(dir.path())
        .args(["compress", "--", "-file"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let mut restored = Vec::new();
    zipper::decompress(output.stdout.as_slice(), &mut restored, None).unwrap();
    assert_eq!(restored, b"dash");
}

#[cfg(unix)]
#[test]
fn interoperates_with_system_gzip_in_both_directions() {
    let data = b"gzip interoperability\n";
    let mut compressor = zipper();
    compressor.arg("compress");
    let encoded = piped(compressor, data);
    assert!(encoded.status.success());
    let mut gzip = Command::new("gzip");
    gzip.args(["-d", "-c"]);
    let decoded = piped(gzip, &encoded.stdout);
    assert!(decoded.status.success());
    assert_eq!(decoded.stdout, data);
    let mut gzip = Command::new("gzip");
    gzip.args(["-n", "-c"]);
    let encoded = piped(gzip, data);
    assert!(encoded.status.success());
    let mut decoder = zipper();
    decoder.arg("decompress");
    let decoded = piped(decoder, &encoded.stdout);
    assert!(decoded.status.success());
    assert_eq!(decoded.stdout, data);
}
