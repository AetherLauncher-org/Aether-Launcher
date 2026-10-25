//! 基于 SHA-1 的下载内容校验。
//!
//! [`HashingWriter`] 在写入的同时计算摘要，续传时可用
//! [`HashingWriter::absorb_existing`] 把已有文件前缀纳入哈希；
//! [`verify_reader`] 把流读完并校验大小与 SHA-1。

use std::io::{self, Read, Write};

use sha1::{Digest, Sha1};
use thiserror::Error;

const HEX: &[u8; 16] = b"0123456789abcdef";

/// 把字节序列编码为小写十六进制字符串。
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[usize::from(byte >> 4)] as char);
        out.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    out
}

/// 校验失败的原因。
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HashMismatch {
    /// 字节数与期望不符。
    #[error("size mismatch: expected {expected} bytes, got {actual}")]
    Size {
        /// 期望的字节数。
        expected: u64,
        /// 实际读到的字节数。
        actual: u64,
    },
    /// SHA-1 摘要与期望不符。
    #[error("SHA-1 mismatch: expected {expected}, got {actual}")]
    Sha1 {
        /// 期望的小写十六进制摘要。
        expected: String,
        /// 实际算出的小写十六进制摘要。
        actual: String,
    },
}

/// [`verify_reader`] 的失败类型：读取错误或校验不匹配。
#[derive(Debug, Error)]
pub enum VerifyError {
    /// 校验过程中底层读取失败。
    #[error("failed to read while verifying: {0}")]
    Io(#[from] io::Error),
    /// 大小或 SHA-1 校验未通过。
    #[error(transparent)]
    Mismatch(#[from] HashMismatch),
}

/// 写入的同时计算 SHA-1 的包装写入器。
///
/// 哈希内容是「吸收的前缀 + 随后写入的字节」的拼接，`total_bytes` 同样统计两者。
pub struct HashingWriter<W: Write> {
    inner: W,
    hasher: Sha1,
    total_bytes: u64,
}

impl<W: Write> HashingWriter<W> {
    /// 创建包装 `inner` 的写入器，初始计数为 0，哈希为空。
    #[must_use]
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            hasher: Sha1::new(),
            total_bytes: 0,
        }
    }

    /// 把 `reader` 的全部字节读入哈希与计数，**不**写入底层写入器。
    ///
    /// 用于续传场景：把磁盘上已有文件的前缀吸收进来，使摘要与完整文件一致。
    /// 返回吸收的字节数。
    pub fn absorb_existing<R: Read>(&mut self, mut reader: R) -> io::Result<u64> {
        let mut absorbed = 0_u64;
        let mut buf = [0_u8; 64 * 1024];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            self.hasher.update(&buf[..n]);
            absorbed += n as u64;
        }
        self.total_bytes += absorbed;
        Ok(absorbed)
    }

    /// 已处理的总字节数，含吸收的前缀。
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// 结束哈希，返回（底层写入器, 总字节数, 小写十六进制 SHA-1）。
    pub fn finish(self) -> (W, u64, String) {
        let Self {
            inner,
            hasher,
            total_bytes,
        } = self;
        (inner, total_bytes, hex(&hasher.finalize()))
    }
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        self.total_bytes += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// 从 `reader` 读完全部字节，校验大小与 SHA-1；通过则返回实际字节数。
///
/// 先比字节数，再比摘要；读取错误以 [`VerifyError::Io`] 原样传出。
pub fn verify_reader<R: Read>(
    mut reader: R,
    expected_size: u64,
    expected_sha1: &str,
) -> Result<u64, VerifyError> {
    let mut hasher = Sha1::new();
    let mut total = 0_u64;
    let mut buf = [0_u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total += n as u64;
    }
    if total != expected_size {
        return Err(HashMismatch::Size {
            expected: expected_size,
            actual: total,
        }
        .into());
    }
    let actual = hex(&hasher.finalize());
    if actual != expected_sha1 {
        return Err(HashMismatch::Sha1 {
            expected: expected_sha1.to_owned(),
            actual,
        }
        .into());
    }
    Ok(total)
}

#[cfg(test)]
#[path = "hash_tests.rs"]
mod tests;
