//! Instance transaction and backend-only source selection.
//!
//! A Modrinth pack is installed either from a project id (the latest compatible `.mrpack`) or
//! from a local `.mrpack` chosen through the host's native picker. No filesystem path or URL is
//! ever accepted from a frontend: the project id is a plain identifier and the local path stays
//! inside this operation.
//!
//! The instance is created only after the vanilla game (and a loader, when the pack declares one)
//! has installed, and is kept incomplete until every pack file is materialized and the instance
//! is activated. A failure at any step removes the half-created instance directory again.

use std::fs;
use std::io::{Read, Seek};
use std::path::Path;

use aether_content::browse;
use aether_download::transport::http::UreqKernel;
use aether_download::transport::KernelHttpTransport;
use aether_download::CancellationToken;

use crate::identify::{ContentNet, UreqNet};
use crate::install::{ActiveInstall, HostReporter, InstallOptions, InstallState, CANCELLED};
use crate::instances::InstanceSummary;
use crate::Host;

use super::net::PackNet;
use super::{check_ancestors, FileSource, Pack};

/// Install a Modrinth pack into a brand-new instance.
///
/// `project` names a Modrinth project whose latest `.mrpack` is fetched and verified; `None`
/// opens the host's native file picker for a local `.mrpack`. `name` is the new instance's
/// display name.
///
/// # Errors
/// Returns a sentence safe to show for validation, download, verification or install failures.
/// Dismissing the picker returns [`CANCELLED`], like every other aborted install.
pub fn install(
    host: &dyn Host,
    state: &InstallState,
    project: Option<&str>,
    name: &str,
    optional: bool,
) -> Result<InstanceSummary, String> {
    let cancel = state.begin()?;
    let _active = ActiveInstall(state);
    let source = PackNet::new(cancel.clone());
    install_with(host, &UreqNet, &source, project, name, optional, &cancel)
}

/// The testable core: the network and the payload transport are injected.
fn install_with(
    host: &dyn Host,
    net: &dyn ContentNet,
    source: &dyn FileSource,
    project: Option<&str>,
    name: &str,
    optional: bool,
    cancel: &CancellationToken,
) -> Result<InstanceSummary, String> {
    let base = host.data_dir()?;
    fs::create_dir_all(&base).map_err(|_| "无法准备数据目录".to_owned())?;
    crate::install::check_instance_name(&base, name)?;

    match project {
        Some(project) => {
            let archive = fetch_archive(net, source, project, &base, cancel)?;
            install_pack(host, Pack::open(archive)?, source, name, optional, cancel)
        }
        None => {
            let path = host
                .pick_file("Modrinth 整合包", &["mrpack"])?
                .ok_or_else(|| CANCELLED.to_owned())?;
            let meta = fs::symlink_metadata(&path).map_err(|_| "无法读取所选整合包".to_owned())?;
            if !meta.is_file()
                || meta.file_type().is_symlink()
                || path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_none_or(|ext| !ext.eq_ignore_ascii_case("mrpack"))
            {
                return Err("请选择普通 .mrpack 文件".to_owned());
            }
            let file = fs::File::open(path).map_err(|_| "无法读取所选整合包".to_owned())?;
            install_pack(host, Pack::open(file)?, source, name, optional, cancel)
        }
    }
}

/// Fetch the latest `.mrpack` for `project`, spool it to a private file and verify it against the
/// hashes the platform listed, before handing it back to be inspected as a ZIP.
fn fetch_archive(
    net: &dyn ContentNet,
    source: &dyn FileSource,
    project: &str,
    base: &Path,
    cancel: &CancellationToken,
) -> Result<fs::File, String> {
    let url = browse::versions_url(project, None, &[]).map_err(|error| error.to_string())?;
    let versions = browse::parse_versions(&net.get_text(&url)?).map_err(|error| error.to_string())?;
    let (_version, file) = browse::pick_modpack_version(&versions)
        .ok_or_else(|| "项目没有带完整哈希的 mrpack 文件".to_owned())?;

    // The index's hash is the trust anchor: the archive is checked against it before parsing.
    let spec = aether_content::modpack::File {
        path: file.filename.clone(),
        file_size: file.size,
        downloads: vec![file.url.clone()],
        env: None,
        hashes: aether_content::modpack::Hashes {
            sha1: file.sha1.clone(),
            sha512: file.sha512.clone().ok_or_else(|| "缺少 SHA-512".to_owned())?,
        },
    };
    if spec.file_size > super::MAX_ARCHIVE_BYTES {
        return Err("整合包超过大小上限".to_owned());
    }

    let staging = base.join(format!(".mrpack-{}", aether_core::InstanceId::new()));
    check_ancestors(base)?;
    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .read(true)
        .open(&staging)
        .map_err(|_| "无法暂存整合包".to_owned())?;

    let result = (|| {
        super::copy_verified(source.open(&file.url)?, &mut output, &spec, cancel)?;
        output
            .rewind()
            .map_err(|_| "无法读取整合包".to_owned())?;
        Ok(output)
    })();

    let cleanup = fs::remove_file(&staging).map_err(|_| "无法清理整合包暂存文件".to_owned());
    match result {
        Ok(file) => {
            cleanup?;
            Ok(file)
        }
        Err(error) => {
            let _ = cleanup;
            Err(error)
        }
    }
}

/// Install a validated pack into a fresh instance: game + loader first, then pack files, then
/// activation. The instance directory is removed again if anything after its creation fails.
fn install_pack<R: Read + Seek>(
    host: &dyn Host,
    mut pack: Pack<R>,
    source: &dyn FileSource,
    name: &str,
    optional: bool,
    cancel: &CancellationToken,
) -> Result<InstanceSummary, String> {
    let base = host.data_dir()?;
    let index = pack.index();
    let game = index
        .dependencies
        .get("minecraft")
        .ok_or_else(|| "整合包没有声明 Minecraft 版本".to_owned())?;
    let transport = KernelHttpTransport::new(UreqKernel::new());
    let reporter = HostReporter(host);

    let version = match index
        .dependencies
        .iter()
        .find(|(key, _)| key.as_str() != "minecraft")
    {
        Some((loader, loader_version)) => {
            let kind = crate::modloader::parse_kind(
                loader.strip_suffix("-loader").unwrap_or(loader),
            )
            .ok_or_else(|| "整合包使用了不支持的加载器".to_owned())?;
            crate::modloader::execute::prepare_with(
                &transport,
                crate::modloader::execute::InstallRequest {
                    base: &base,
                    kind,
                    game_version: game,
                    loader_version,
                    name,
                },
                cancel,
                &reporter,
            )?
        }
        None => {
            crate::install::install_game_files(
                &transport,
                &base,
                game,
                InstallOptions::default(),
                cancel,
                &reporter,
            )?;
            game.clone()
        }
    };
    if cancel.is_cancelled() {
        return Err(CANCELLED.to_owned());
    }

    // Create the instance last, still incomplete, so a later failure never leaves a
    // ready-but-broken instance behind.
    let instance = aether_storage::create_instance(&base, name, &version)
        .map_err(|_| "无法创建整合包实例".to_owned())?;
    let root = instance.root.clone();

    let result = (|| {
        let game_dir = aether_storage::manage::game_directory(&instance);
        // The skeleton wrote a default `options.txt`; the pack owns the directory from here on.
        fs::remove_dir(&game_dir).map_err(|_| "新实例目录不是空目录".to_owned())?;
        pack.materialize(&game_dir, source, optional)?;
        if cancel.is_cancelled() {
            return Err(CANCELLED.to_owned());
        }
        crate::launch::activate_instance(&base, &base.join("game"), instance.id)?;
        let ready = aether_storage::InstanceStore::open(&base)
            .and_then(|store| store.load(instance.id))
            .map_err(|_| "无法读取新实例".to_owned())?;
        Ok(InstanceSummary::from_instance(&ready))
    })();

    if result.is_err() {
        check_ancestors(&root)?;
        fs::remove_dir_all(root).map_err(|_| "安装失败，半成品实例清理失败".to_owned())?;
    }
    result
}

#[cfg(test)]
#[path = "install_tests.rs"]
mod install_tests;
