use std::io::{self, Cursor};

use super::*;

const ABC_SHA1: &str = "a9993e364706816aba3e25717850c26c9cd0d89d";

/// 每次只接受 1 个字节的底层写入器，用于验证只哈希实际接受的字节。
struct OneByteAtATimeWriter(Vec<u8>);

impl Write for OneByteAtATimeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match buf.first() {
            Some(&byte) => {
                self.0.push(byte);
                Ok(1)
            }
            None => Ok(0),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// `read` 永远失败的读取器。
struct FailingReader;

impl Read for FailingReader {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("boom"))
    }
}

#[test]
fn hashing_writer_finishes_with_sha1_of_written_bytes() {
    let mut writer = HashingWriter::new(Vec::new());
    let written = writer.write(b"abc").expect("write should succeed");
    assert_eq!(written, 3);
    let (inner, total, digest) = writer.finish();
    assert_eq!(inner.as_slice(), b"abc");
    assert_eq!(total, 3);
    assert_eq!(digest, ABC_SHA1);
}

#[test]
fn write_hashes_only_bytes_the_inner_writer_accepted() {
    let mut writer = HashingWriter::new(OneByteAtATimeWriter(Vec::new()));
    assert_eq!(writer.write(b"abc").expect("first write"), 1);
    assert_eq!(writer.write(b"bc").expect("second write"), 1);
    assert_eq!(writer.write(b"c").expect("third write"), 1);
    let (inner, total, digest) = writer.finish();
    assert_eq!(inner.0.as_slice(), b"abc");
    assert_eq!(total, 3);
    assert_eq!(digest, ABC_SHA1);
}

#[test]
fn absorb_existing_prefixes_the_digest_without_writing() {
    let mut writer = HashingWriter::new(Vec::new());
    let absorbed = writer
        .absorb_existing(Cursor::new(b"ab".to_vec()))
        .expect("absorb should succeed");
    assert_eq!(absorbed, 2);
    writer.write_all(b"c").expect("write should succeed");
    let (inner, total, digest) = writer.finish();
    assert_eq!(inner.as_slice(), b"c");
    assert_eq!(total, 3);
    assert_eq!(digest, ABC_SHA1);
}

#[test]
fn total_bytes_counts_absorbed_prefix_before_writes() {
    let mut writer = HashingWriter::new(Vec::new());
    assert_eq!(writer.total_bytes(), 0);
    writer
        .absorb_existing(Cursor::new(b"ab".to_vec()))
        .expect("absorb should succeed");
    assert_eq!(writer.total_bytes(), 2);
}

#[test]
fn verify_reader_accepts_matching_size_and_digest() {
    let reader = Cursor::new(b"abc".to_vec());
    match verify_reader(reader, 3, ABC_SHA1) {
        Ok(bytes) => assert_eq!(bytes, 3),
        Err(err) => panic!("expected success, got {err:?}"),
    }
}

#[test]
fn verify_reader_rejects_size_mismatch() {
    let reader = Cursor::new(b"abc".to_vec());
    let result = verify_reader(reader, 5, ABC_SHA1);
    assert!(matches!(
        result,
        Err(VerifyError::Mismatch(HashMismatch::Size {
            expected: 5,
            actual: 3
        }))
    ));
}

#[test]
fn verify_reader_rejects_sha1_mismatch() {
    let reader = Cursor::new(b"abc".to_vec());
    let expected = "0".repeat(40);
    match verify_reader(reader, 3, &expected) {
        Err(VerifyError::Mismatch(HashMismatch::Sha1 { expected, actual })) => {
            assert_eq!(expected, "0".repeat(40));
            assert_eq!(actual, ABC_SHA1);
        }
        other => panic!("expected SHA-1 mismatch, got {other:?}"),
    }
}

#[test]
fn verify_reader_propagates_read_errors() {
    let result = verify_reader(FailingReader, 3, ABC_SHA1);
    assert!(matches!(result, Err(VerifyError::Io(_))));
}
