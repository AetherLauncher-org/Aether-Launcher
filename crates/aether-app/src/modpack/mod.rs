//! Bounded ZIP inspection and verified materialization into a new private directory.
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, Write};
use std::path::Path;

use aether_content::modpack::{
    self, Index, MAX_FILE_BYTES, MAX_INDEX_BYTES, MAX_TOTAL_BYTES, Side,
};
use aether_download::CancellationToken;
use sha1::{Digest, Sha1};
use sha2::Sha512;
use zip::ZipArchive;

pub const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 20_000;

/// A validated archive retains the same reader for inspection and extraction.
/// No caller can substitute an unvalidated archive or alter the plan.
pub struct Pack<R> {
    archive: ZipArchive<R>,
    index: Index,
    overrides: BTreeMap<String, String>,
}

fn invalid() -> String {
    "整合包 ZIP 无效或超出安全限制".to_owned()
}

fn check_cancel(cancel: &CancellationToken) -> Result<(), String> {
    if cancel.is_cancelled() {
        Err(crate::install::CANCELLED.to_owned())
    } else {
        Ok(())
    }
}

impl<R: Read + Seek> Pack<R> {
    /// Validate every entry, including ignored server overrides, before extracting anything.
    ///
    /// # Errors
    /// Rejects unsafe names, links, duplicate names, collisions and oversized archives.
    pub fn open(mut reader: R) -> Result<Self, String> {
        if reader
            .seek(std::io::SeekFrom::End(0))
            .map_err(|_| invalid())?
            > MAX_ARCHIVE_BYTES
        {
            return Err(invalid());
        }
        reader.rewind().map_err(|_| invalid())?;
        let mut archive = ZipArchive::new(reader).map_err(|_| invalid())?;
        if archive.len() > MAX_ENTRIES {
            return Err(invalid());
        }
        let mut names = BTreeSet::new();
        let mut regular = BTreeSet::new();
        let mut overrides = BTreeMap::new();
        let mut client = BTreeMap::new();
        let mut total = 0u64;
        for i in 0..archive.len() {
            let entry = archive.by_index(i).map_err(|_| invalid())?;
            let name = entry.name().map_err(|_| invalid())?;
            let name = name.as_ref();
            let path = name.strip_suffix('/').unwrap_or(name);
            if !modpack::safe_path(path) || !names.insert(path.to_lowercase()) || entry.encrypted()
            {
                return Err(invalid());
            }
            if let Some(mode) = entry.unix_mode() {
                let kind = mode & 0o170000;
                if kind != 0 && kind != if entry.is_dir() { 0o040000 } else { 0o100000 } {
                    return Err(invalid());
                }
            }
            total = total.checked_add(entry.size()).ok_or_else(invalid)?;
            if total > MAX_TOTAL_BYTES || entry.size() > MAX_FILE_BYTES {
                return Err(invalid());
            }
            if entry.is_dir() {
                continue;
            }
            regular.insert(path.to_lowercase());
            for (prefix, targets) in [
                ("overrides/", &mut overrides),
                ("client-overrides/", &mut client),
            ] {
                if let Some(target) = name.strip_prefix(prefix) {
                    if !modpack::safe_path(target) {
                        return Err(invalid());
                    }
                    targets.insert(target.to_owned(), name.to_owned());
                }
            }
        }
        for name in &names {
            reject_parent_file(name, &regular)?;
        }
        let index = {
            let entry = archive
                .by_name("modrinth.index.json")
                .map_err(|_| invalid())?;
            if entry.size() > MAX_INDEX_BYTES as u64 {
                return Err(invalid());
            }
            let mut bytes = Vec::new();
            entry
                .take(MAX_INDEX_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| invalid())?;
            modpack::parse(&bytes).map_err(|e| e.to_string())?
        };
        let download_total: u64 = index.files.iter().map(|file| file.file_size).sum();
        if total.checked_add(download_total).is_none_or(|size| size > MAX_TOTAL_BYTES) {
            return Err(invalid());
        }
        // Resolve the specified client layer before touching disk, never overwrite a file.
        overrides.extend(client);
        let mut targets = BTreeSet::new();
        for file in &index.files {
            if file
                .env
                .as_ref()
                .is_some_and(|env| env.client == Side::Unsupported)
            {
                continue;
            }
            if !targets.insert(file.path.to_lowercase()) {
                return Err(invalid());
            }
        }
        for path in overrides.keys() {
            if !targets.insert(path.to_lowercase()) {
                return Err("整合包下载与覆盖层目标冲突".to_owned());
            }
        }
        for path in &targets {
            reject_parent_file(path, &targets)?;
        }
        Ok(Self {
            archive,
            index,
            overrides,
        })
    }

    #[must_use]
    pub fn index(&self) -> &Index {
        &self.index
    }

    /// Materialize into a directory that must not exist. On failure remove only that directory.
    /// The parent is launcher-owned and must not be modified concurrently by other processes.
    /// Optional client files are controlled explicitly by the caller, never silently selected.
    ///
    /// # Errors
    /// Returns read, hash, cancellation, collision, or rollback failures without exposing paths.
    pub fn materialize(
        &mut self,
        root: &Path,
        source: &dyn FileSource,
        optional: bool,
    ) -> Result<(), String> {
        self.materialize_with_cancel(root, source, optional, &CancellationToken::new())
    }

    /// Materialize with cancellation between entries and read chunks.
    ///
    /// # Errors
    /// Returns validation, I/O, cancellation or cleanup failures.
    pub fn materialize_with_cancel(
        &mut self,
        root: &Path,
        source: &dyn FileSource,
        optional: bool,
        cancel: &CancellationToken,
    ) -> Result<(), String> {
        check_cancel(cancel)?;
        let parent = root.parent().ok_or_else(invalid)?;
        check_ancestors(parent)?;
        fs::create_dir(root).map_err(|_| "整合包目标必须是新的目录".to_owned())?;
        let result = self.write_files(root, source, optional, cancel);
        if result.is_err() && fs::remove_dir_all(root).is_err() {
            return Err("整合包安装失败，清理临时目录也失败，需要手动检查".to_owned());
        }
        result
    }

    fn write_files(
        &mut self,
        root: &Path,
        source: &dyn FileSource,
        optional: bool,
        cancel: &CancellationToken,
    ) -> Result<(), String> {
        for file in &self.index.files {
            check_cancel(cancel)?;
            if file.env.as_ref().is_some_and(|env| {
                env.client == Side::Unsupported || (env.client == Side::Optional && !optional)
            }) {
                continue;
            }
            let mut last_error = "整合包文件下载失败".to_owned();
            let mut downloaded = false;
            for url in &file.downloads {
                check_cancel(cancel)?;
                let mut owned = false;
                let attempt = (|| {
                    let reader = source.open(url)?;
                    let mut output = create_file(root, &file.path)?;
                    owned = true;
                    copy_verified(reader, &mut output, file, cancel)
                })();
                match attempt {
                    Ok(()) => {
                        downloaded = true;
                        break;
                    }
                    Err(error) => {
                        last_error = error;
                        let target = root.join(&file.path);
                        if owned {
                            fs::remove_file(target)
                                .map_err(|_| "无法清理未通过校验的文件".to_owned())?;
                        }
                    }
                }
            }
            if !downloaded {
                return Err(last_error);
            }
        }
        let mut total = 0u64;
        for (path, name) in &self.overrides {
            check_cancel(cancel)?;
            let entry = self.archive.by_name(name).map_err(|_| invalid())?;
            let size = entry.size();
            let mut output = create_file(root, path)?;
            let mut input = entry.take(size + 1);
            let mut copied = 0u64;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                check_cancel(cancel)?;
                let count = input.read(&mut buffer).map_err(|_| invalid())?;
                if count == 0 { break; }
                copied += count as u64;
                if copied > size { return Err(invalid()); }
                output.write_all(&buffer[..count]).map_err(|_| invalid())?;
            }
            total = total.checked_add(copied).ok_or_else(invalid)?;
            if copied != size || total > MAX_TOTAL_BYTES {
                return Err(invalid());
            }
            output.sync_all().map_err(|_| invalid())?;
        }
        Ok(())
    }
}

/// Pack payload transport, independently replaceable by an in-memory test source.
/// Implementations must check every redirect before issuing it and bound response time.
pub trait FileSource {
    fn open(&self, url: &str) -> Result<Box<dyn Read>, String>;
}

fn reject_parent_file(path: &str, files: &BTreeSet<String>) -> Result<(), String> {
    let mut rest = path;
    while let Some((parent, _)) = rest.rsplit_once('/') {
        if files.contains(parent) {
            return Err(invalid());
        }
        rest = parent;
    }
    Ok(())
}

fn check_ancestors(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors().filter(|p| !p.as_os_str().is_empty()) {
        let meta = fs::symlink_metadata(ancestor).map_err(|_| invalid())?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(invalid());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err(invalid());
            }
        }
    }
    Ok(())
}

fn create_file(root: &Path, path: &str) -> Result<fs::File, String> {
    if !modpack::safe_path(path) {
        return Err(invalid());
    }
    let target = root.join(path);
    let parent = target.parent().ok_or_else(invalid)?;
    let mut current = root.to_path_buf();
    let parts: Vec<_> = path.split('/').collect();
    for part in &parts[..parts.len() - 1] {
        current.push(part);
        match fs::create_dir(&current) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(invalid()),
        }
        check_ancestors(&current)?;
    }
    check_ancestors(parent)?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|_| "整合包文件已存在或无法创建".to_owned())
}

fn copy_verified(
    mut input: Box<dyn Read>,
    output: &mut fs::File,
    file: &modpack::File,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let (mut sha1, mut sha512) = (Sha1::new(), Sha512::new());
    let mut count = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        check_cancel(cancel)?;
        let n = match input.read(&mut buffer) {
            Ok(n) => n,
            Err(_) => {
                check_cancel(cancel)?;
                return Err("读取整合包文件失败".to_owned());
            }
        };
        check_cancel(cancel)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > file.file_size {
            return Err("整合包文件超过声明大小".to_owned());
        }
        sha1.update(&buffer[..n]);
        sha512.update(&buffer[..n]);
        output
            .write_all(&buffer[..n])
            .map_err(|_| "写入整合包文件失败".to_owned())?;
    }
    if count != file.file_size
        || !format!("{:x}", sha1.finalize()).eq_ignore_ascii_case(&file.hashes.sha1)
        || !format!("{:x}", sha512.finalize()).eq_ignore_ascii_case(&file.hashes.sha512)
    {
        return Err("整合包文件大小或 SHA-1 / SHA-512 校验失败".to_owned());
    }
    output
        .sync_all()
        .map_err(|_| "保存整合包文件失败".to_owned())
}

#[cfg(test)]
mod modpack_tests;

mod install;
mod net;
pub use install::install;
