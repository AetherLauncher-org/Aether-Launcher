//! Backend-only source selection and staged new-instance installation.
use super::net::PackNet;
use super::{FileSource, Pack, check_ancestors, check_cancel};
use crate::Host;
use crate::identify::{ContentNet, UreqNet};
use crate::install::{ActiveInstall, CANCELLED, HostReporter, InstallOptions, InstallState};
use crate::instances::InstanceSummary;
use aether_content::{browse, modpack::Index};
use aether_core::{InstanceId, InstanceState};
use aether_download::CancellationToken;
use aether_download::transport::KernelHttpTransport;
use aether_download::transport::http::UreqKernel;
use std::fs;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

/// Install a new instance without accepting filesystem paths or URLs from IPC.
/// # Errors
/// Returns a safe install error, or `cancelled` on picker dismissal.
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
    install_with(
        &Services {
            host,
            net: &UreqNet,
            source: &source,
            game: &Game,
        },
        project,
        name,
        optional,
        &cancel,
    )
}
trait GameLifecycle {
    fn prepare(
        &self,
        host: &dyn Host,
        base: &Path,
        index: &Index,
        name: &str,
        cancel: &CancellationToken,
    ) -> Result<String, String>;
    fn activate(&self, store: &Path, game: &Path, id: InstanceId) -> Result<(), String>;
}
struct Game;
impl GameLifecycle for Game {
    fn prepare(
        &self,
        host: &dyn Host,
        base: &Path,
        index: &Index,
        name: &str,
        cancel: &CancellationToken,
    ) -> Result<String, String> {
        let game = index
            .dependencies
            .get("minecraft")
            .ok_or_else(|| "整合包没有声明 Minecraft 版本".to_owned())?;
        let transport = KernelHttpTransport::new(UreqKernel::new());
        let reporter = HostReporter(host);
        match index
            .dependencies
            .iter()
            .find(|(key, _)| key.as_str() != "minecraft")
        {
            Some((loader, loader_version)) => {
                let kind =
                    crate::modloader::parse_kind(loader.strip_suffix("-loader").unwrap_or(loader))
                        .ok_or_else(|| "整合包使用了不支持的加载器".to_owned())?;
                crate::modloader::execute::prepare_with(
                    &transport,
                    crate::modloader::execute::InstallRequest {
                        base,
                        kind,
                        game_version: game,
                        loader_version,
                        name,
                    },
                    cancel,
                    &reporter,
                )
            }
            None => {
                crate::install::install_game_files(
                    &transport,
                    base,
                    game,
                    InstallOptions::default(),
                    cancel,
                    &reporter,
                )?;
                Ok(game.clone())
            }
        }
    }
    fn activate(&self, store: &Path, game: &Path, id: InstanceId) -> Result<(), String> {
        crate::launch::activate_instance(store, game, id)
    }
}
struct Services<'a> {
    host: &'a dyn Host,
    net: &'a dyn ContentNet,
    source: &'a dyn FileSource,
    game: &'a dyn GameLifecycle,
}
fn install_with(
    services: &Services<'_>,
    project: Option<&str>,
    name: &str,
    optional: bool,
    cancel: &CancellationToken,
) -> Result<InstanceSummary, String> {
    check_cancel(cancel)?;
    let base = services.host.data_dir()?;
    fs::create_dir_all(&base).map_err(|_| "无法准备数据目录".to_owned())?;
    check_ancestors(&base)?;
    crate::install::check_instance_name(&base, name)?;
    match project {
        Some(project) => {
            let (archive, staging) =
                fetch_archive(services.net, services.source, project, &base, cancel)?;
            // Include Pack::open failures; the reader closes before Windows cleanup.
            let result =
                (|| install_pack(services, Pack::open(archive)?, name, optional, cancel))();
            let cleanup = fs::remove_file(staging).map_err(|_| "无法清理整合包暂存文件".to_owned());
            match result {
                Ok(value) => {
                    let _ = cleanup;
                    Ok(value)
                }
                Err(error) => {
                    cleanup?;
                    Err(error)
                }
            }
        }
        None => {
            let path = services
                .host
                .pick_file("Modrinth 整合包", &["mrpack"])?
                .ok_or_else(|| CANCELLED.to_owned())?;
            check_cancel(cancel)?;
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
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err("请选择普通 .mrpack 文件".to_owned());
                }
            }
            let file = fs::File::open(path).map_err(|_| "无法读取所选整合包".to_owned())?;
            install_pack(services, Pack::open(file)?, name, optional, cancel)
        }
    }
}
/// Caller closes the returned reader before deleting its staging path.
fn fetch_archive(
    net: &dyn ContentNet,
    source: &dyn FileSource,
    project: &str,
    base: &Path,
    cancel: &CancellationToken,
) -> Result<(fs::File, PathBuf), String> {
    check_cancel(cancel)?;
    let url = browse::versions_url(project, None, &[]).map_err(|error| error.to_string())?;
    let versions =
        browse::parse_versions(&net.get_text(&url)?).map_err(|error| error.to_string())?;
    check_cancel(cancel)?;
    let (_, file) = browse::pick_modpack_version(&versions)
        .ok_or_else(|| "项目没有带完整哈希的 mrpack 文件".to_owned())?;
    // The official platform response anchors the archive, not the index inside it.
    let spec = aether_content::modpack::File {
        path: file.filename.clone(),
        file_size: file.size,
        downloads: vec![file.url.clone()],
        env: None,
        hashes: aether_content::modpack::Hashes {
            sha1: file.sha1.clone(),
            sha512: file
                .sha512
                .clone()
                .ok_or_else(|| "缺少 SHA-512".to_owned())?,
        },
    };
    if spec.file_size > super::MAX_ARCHIVE_BYTES {
        return Err("整合包超过大小上限".to_owned());
    }
    let staging = base.join(format!(".mrpack-{}", InstanceId::new()));
    check_ancestors(base)?;
    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .read(true)
        .open(&staging)
        .map_err(|_| "无法暂存整合包".to_owned())?;
    let result = (|| {
        super::copy_verified(source.open(&file.url)?, &mut output, &spec, cancel)?;
        output.rewind().map_err(|_| "无法读取整合包".to_owned())?;
        check_cancel(cancel)
    })();
    match result {
        Ok(()) => Ok((output, staging)),
        Err(error) => {
            drop(output);
            fs::remove_file(staging).map_err(|_| "无法清理整合包暂存文件".to_owned())?;
            Err(error)
        }
    }
}
fn install_pack<R: Read + Seek>(
    services: &Services<'_>,
    mut pack: Pack<R>,
    name: &str,
    optional: bool,
    cancel: &CancellationToken,
) -> Result<InstanceSummary, String> {
    check_cancel(cancel)?;
    let base = services.host.data_dir()?;
    let version = services
        .game
        .prepare(services.host, &base, pack.index(), name, cancel)?;
    check_cancel(cancel)?;
    check_ancestors(&base)?;
    let staging = base.join(format!(".mrpack-instance-{}", InstanceId::new()));
    fs::create_dir(&staging).map_err(|_| "无法准备整合包实例".to_owned())?;
    let result = (|| {
        // Defaults and the incomplete record stay outside the visible instance store.
        let instance = aether_storage::create_instance(&staging, name, &version)
            .map_err(|_| "无法创建整合包实例".to_owned())?;
        let game_dir = aether_storage::manage::game_directory(&instance);
        let packed = instance.root.join("pack");
        pack.materialize_with_cancel(&packed, services.source, optional, cancel)?;
        let options = packed.join("options.txt");
        if !options
            .try_exists()
            .map_err(|_| "无法读取整合包设置".to_owned())?
        {
            fs::rename(game_dir.join("options.txt"), &options)
                .map_err(|_| "无法保留实例默认设置".to_owned())?;
        }
        // Only transaction-owned directories are replaced; pack settings take priority.
        check_ancestors(&game_dir)?;
        fs::remove_dir_all(&game_dir).map_err(|_| "无法替换新实例目录".to_owned())?;
        fs::rename(&packed, &game_dir).map_err(|_| "无法保存整合包文件".to_owned())?;
        check_cancel(cancel)?;
        services
            .game
            .activate(&staging, &base.join("game"), instance.id)?;
        check_cancel(cancel)?;
        let mut ready = aether_storage::InstanceStore::open(&staging)
            .and_then(|store| store.load(instance.id))
            .map_err(|_| "无法读取新实例".to_owned())?;
        if ready.state != InstanceState::Ready {
            return Err("整合包实例未激活".to_owned());
        }
        crate::install::check_instance_name(&base, name)?;
        let instances = base.join("instances");
        fs::create_dir_all(&instances).map_err(|_| "无法准备实例目录".to_owned())?;
        check_ancestors(&instances)?;
        let destination = fs::canonicalize(&instances)
            .map_err(|_| "无法读取实例目录".to_owned())?
            .join(instance.id.to_string());
        if destination
            .try_exists()
            .map_err(|_| "无法检查实例目录".to_owned())?
        {
            return Err("实例目录已存在".to_owned());
        }
        ready.root = destination.clone();
        aether_storage::write_instance_atomic(&instance.root, &ready)
            .map_err(|_| "无法保存新实例".to_owned())?;
        check_cancel(cancel)?;
        // Single publication point; do not turn success into cancellation after rename.
        fs::rename(&instance.root, destination).map_err(|_| "无法发布整合包实例".to_owned())?;
        Ok(InstanceSummary::from_instance(&ready))
    })();
    let cleanup = check_ancestors(&staging).and_then(|()| {
        fs::remove_dir_all(&staging).map_err(|_| "整合包临时目录清理失败".to_owned())
    });
    match result {
        Ok(summary) => {
            let _ = cleanup;
            Ok(summary)
        }
        Err(error) => {
            cleanup?;
            Err(error)
        }
    }
}
#[cfg(test)]
#[path = "install_tests.rs"]
mod install_tests;
