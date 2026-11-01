//! Native-picked destinations reuse the instance install's version and hash checks, and run
//! inside the same operation machinery: a reserved operation id, progress events, cooperative
//! cancellation, the two-slot queue and a retained kept-files report.
use super::*;

/// Saves a compatible, verified version to a native-picked folder, observing cancellation and
/// progress. The instance supplies compatibility only; its files are not changed.
///
/// Returns `Ok(None)` when the picker is dismissed, `Ok(Some(report))` after the files were
/// kept (or dependencies skipped), and `Err((reason, installed))` for a top-level failure with
/// the files that were already kept.
pub fn save_to_folder_with_control(
    net: &dyn ContentNet,
    host: &dyn Host,
    id: &str,
    project_id: &str,
    project_type: &str,
    with_dependencies: bool,
    control: &dyn InstallControl,
) -> Result<Option<InstallReport>, (String, Vec<InstalledFile>)> {
    let mut report = InstallReport {
        installed: Vec::new(),
        skipped: Vec::new(),
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        save_controlled_inner(
            net,
            host,
            id,
            project_id,
            project_type,
            with_dependencies,
            control,
            &mut report,
        )
    }))
    .unwrap_or_else(|_| Err("the content install task failed".to_owned()));
    match result {
        Ok(true) => Ok(Some(report)),
        Ok(false) => Ok(None),
        Err(error) => Err((error, report.installed)),
    }
}

#[allow(clippy::too_many_arguments)]
fn save_controlled_inner(
    net: &dyn ContentNet,
    host: &dyn Host,
    id: &str,
    project_id: &str,
    project_type: &str,
    with_dependencies: bool,
    control: &dyn InstallControl,
    report: &mut InstallReport,
) -> Result<bool, String> {
    check_cancel(control)?;
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
        return Ok(false);
    };
    crate::cachefile::check_directory(&folder)
        .map_err(|_| "the selected folder is not a plain directory".to_owned())?;

    let mut discovered = BTreeSet::from([project_id.to_owned()]);
    control.discovered(discovered.len());
    let mut seen = BTreeSet::new();
    let mut queue = vec![(project_id.to_owned(), 0usize)];
    while let Some((project, depth)) = queue.pop() {
        check_cancel(control)?;
        if !seen.insert(project.clone()) {
            continue;
        }
        if report.installed.len() >= MAX_FILES_PER_INSTALL {
            report.skipped.push(SkippedProject {
                project_id: project,
                reason: "too many files for one save".to_owned(),
            });
            continue;
        }
        match install_one(net, &project, &game_version, loader, &folder, extension, control) {
            Ok((file, dependencies)) => {
                report.installed.push(file);
                control.committed();
                if with_dependencies && kind == ProjectType::Mod && depth < MAX_DEPENDENCY_DEPTH {
                    let known = discovered.len();
                    for dependency in dependencies {
                        if discovered.insert(dependency.clone()) {
                            queue.push((dependency, depth + 1));
                        }
                    }
                    if discovered.len() != known {
                        control.discovered(discovered.len());
                    }
                }
            }
            Err(reason) if depth == 0 || reason == CANCELLED => return Err(reason),
            Err(reason) => report.skipped.push(SkippedProject {
                project_id: project,
                reason,
            }),
        }
    }
    Ok(true)
}
