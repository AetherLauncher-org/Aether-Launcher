//! Native-picked destinations reuse the instance install's version and hash checks.
use super::*;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FolderReport {
    pub files: Vec<String>,
    pub message: String,
}

/// Saves a compatible, verified version to a native-picked folder.
/// The instance supplies compatibility only; its files are not changed.
///
/// # Errors
/// For invalid compatibility, picker, download, checksum or an existing file.
pub fn save_to_folder_with(
    net: &dyn ContentNet,
    host: &dyn Host,
    id: &str,
    project_id: &str,
    project_type: &str,
    with_dependencies: bool,
) -> Result<Option<FolderReport>, String> {
    let (_, instance) = load(host, id)?;
    let kind = ProjectType::parse(project_type).ok_or("unknown content type")?;
    let target = target_of(instance.version.as_str());
    let game_version = target
        .game_version
        .ok_or("cannot tell which game version this instance is")?;
    let (loader, extension) = match kind {
        ProjectType::Mod => (
            Some(target.loader.ok_or(
                "this instance has no mod loader, mods need Fabric, Quilt, Forge or NeoForge",
            )?),
            "jar",
        ),
        ProjectType::ResourcePack | ProjectType::Shader | ProjectType::DataPack => (None, "zip"),
        ProjectType::ModPack => return Err("modpacks cannot be installed here yet".to_owned()),
    };
    let Some(folder) = host.pick_folder("保存到文件夹")? else {
        return Ok(None);
    };
    crate::cachefile::check_directory(&folder)
        .map_err(|_| "the selected folder is not a plain directory".to_owned())?;
    let mut files = Vec::new();
    let mut skipped = 0;
    let mut seen = BTreeSet::new();
    let mut queue = vec![(project_id.to_owned(), 0usize)];
    while let Some((project, depth)) = queue.pop() {
        if !seen.insert(project.clone()) {
            continue;
        }
        if files.len() >= MAX_FILES_PER_INSTALL {
            skipped += 1;
            continue;
        }
        match install_one(net, &project, &game_version, loader, &folder, extension) {
            Ok((file, dependencies)) => {
                files.push(file.file_name);
                if with_dependencies && kind == ProjectType::Mod && depth < MAX_DEPENDENCY_DEPTH {
                    queue.extend(
                        dependencies
                            .into_iter()
                            .map(|dependency| (dependency, depth + 1)),
                    );
                }
            }
            Err(reason) if depth == 0 => return Err(reason),
            Err(_) => skipped += 1,
        }
    }
    let message = if skipped == 0 {
        "已保存到所选文件夹".to_owned()
    } else {
        format!("已保存到所选文件夹；{skipped} 个前置未保存")
    };
    Ok(Some(FolderReport { files, message }))
}

/// Saves using Modrinth. Call off the interface thread.
///
/// # Errors
/// As [`save_to_folder_with`].
pub fn save_to_folder(
    host: &dyn Host,
    id: &str,
    project_id: &str,
    project_type: &str,
    with_dependencies: bool,
) -> Result<Option<FolderReport>, String> {
    save_to_folder_with(
        &UreqNet,
        host,
        id,
        project_id,
        project_type,
        with_dependencies,
    )
}
