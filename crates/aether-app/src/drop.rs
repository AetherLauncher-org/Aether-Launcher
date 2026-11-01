//! Staging and installing native drops. Paths stay in this process; the UI gets a short-lived
//! single-use handle and plain file names. The platform listener is intentionally separate.
use std::{collections::HashMap, fs, io::{Read, Write}, path::{Path, PathBuf}, sync::{Arc, Mutex, atomic::{AtomicU64, Ordering}}, time::{Duration, Instant}};
use aether_core::InstanceId;
use aether_storage::manage::{game_directory, is_plain_name};
use serde::Serialize;
use crate::{host::Host, instances, world};

const LIMIT: u64 = 256 * 1024 * 1024;
const TTL: Duration = Duration::from_secs(120);
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind { Mod, Datapack, Resourcepack, Shader }
#[derive(Clone, Debug, Serialize)]
pub struct Item { pub name: String, pub size: u64, pub kind: Kind }
#[derive(Clone, Debug, Serialize)]
pub struct Preview { pub token: String, pub items: Vec<Item> }
#[derive(Clone)]
struct Source { path: PathBuf, item: Item }
struct Pending { expires: Instant, files: Vec<Source> }
#[derive(Clone, Default)]
pub struct DropState { pending: Arc<Mutex<HashMap<String, Pending>>> }
#[derive(Debug, Serialize)]
pub struct Outcome { pub name: String, pub world: Option<String> }
#[derive(Debug, Serialize)]
pub struct Skipped { pub name: String, pub world: Option<String>, pub reason: String }
#[derive(Debug, Serialize)]
pub struct Report { pub installed: Vec<Outcome>, pub skipped: Vec<Skipped> }

/// Replaceable platform seam. The shipped implementation has no native hookup.
/// A native implementation must deliver paths only to this callback, never to the webview.
pub trait NativeDropListener { fn start(&self, callback: Box<dyn Fn(Vec<PathBuf>) + Send + Sync>); }
pub struct NoNativeDropListener;
impl NativeDropListener for NoNativeDropListener { fn start(&self, _: Box<dyn Fn(Vec<PathBuf>) + Send + Sync>) {} }

fn classify(path: &Path) -> Result<Kind, String> {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    if ext.eq_ignore_ascii_case("jar") { return Ok(Kind::Mod); }
    if !ext.eq_ignore_ascii_case("zip") { return Err("unsupported file type".into()); }
    let file = fs::File::open(path).map_err(|_| "cannot open dropped file")?;
    let zip = zip::ZipArchive::new(file).map_err(|_| "invalid zip file")?;
    if zip.len() > 100_000 { return Err("zip has too many entries".into()); }
    let mut meta = false; let mut data = false; let mut assets = false; let mut shaders = false;
    for name in zip.file_names() {
        let Ok(name) = name else { return Err("unreadable zip entry name".into()) };
        meta |= name == "pack.mcmeta"; data |= name.starts_with("data/"); assets |= name.starts_with("assets/"); shaders |= name.starts_with("shaders/");
    }
    if meta && data { Ok(Kind::Datapack) } else if meta && assets { Ok(Kind::Resourcepack) } else if !meta && shaders { Ok(Kind::Shader) } else { Err("unrecognized zip contents".into()) }
}
fn plain_file(path: &Path) -> Result<fs::Metadata, String> {
    let m = fs::symlink_metadata(path).map_err(|_| "cannot read dropped file")?;
    #[cfg(windows)] { use std::os::windows::fs::MetadataExt; if m.file_attributes() & 0x400 != 0 { return Err("dropped file is a link".into()); } }
    if !m.is_file() || m.file_type().is_symlink() { return Err("dropped item is not a plain file".into()); }
    if m.len() > LIMIT { return Err("dropped file exceeds 256 MiB".into()); }
    Ok(m)
}
/// Accepts paths only from a native listener or a Rust test, never IPC.
pub fn stage(state: &DropState, paths: Vec<PathBuf>) -> Result<Preview, String> { stage_at(state, paths, Instant::now()) }
fn stage_at(state: &DropState, paths: Vec<PathBuf>, now: Instant) -> Result<Preview, String> {
    if paths.is_empty() || paths.len() > 20 { return Err("drop between 1 and 20 files".into()); }
    let mut files = Vec::new();
    for path in paths {
        let name = path.file_name().and_then(|s| s.to_str()).filter(|s| is_plain_name(s)).ok_or("invalid file name")?.to_owned();
        let metadata = plain_file(&path)?;
        let kind = classify(&path).map_err(|e| format!("{name}: {e}"))?;
        files.push(Source { path, item: Item { name, size: metadata.len(), kind } });
    }
    let items = files.iter().map(|f| f.item.clone()).collect();
    let token = InstanceId::new().to_string();
    let mut pending = state.pending.lock().map_err(|_| "drop state unavailable")?;
    pending.retain(|_, p| p.expires > now);
    if pending.len() >= 32 { return Err("too many pending drops".into()); }
    pending.insert(token.clone(), Pending { expires: now + TTL, files });
    Ok(Preview { token, items })
}
fn take(state: &DropState, token: &str, now: Instant) -> Result<Vec<Source>, String> {
    let pending = state.pending.lock().map_err(|_| "drop state unavailable")?.remove(token).ok_or("drop is no longer available")?;
    if pending.expires <= now { return Err("drop expired; drop the files again".into()); }
    Ok(pending.files)
}
/// Installs each file independently; the handle is consumed when installation begins.
pub fn install(host: &dyn Host, state: &DropState, token: &str, id: &str, worlds: &[String]) -> Result<Report, String> {
    let (_, instance) = instances::load(host, id)?;
    if host.is_instance_running(instance.id)? { return Err("stop the game before installing content".into()); }
    if worlds.len() > 100 { return Err("too many worlds".into()); }
    let game = game_directory(&instance);
    let mut dirs = Vec::new();
    for name in worlds { if dirs.iter().any(|(n, _)| n == name) { continue; } dirs.push((name.clone(), world::world_folder(&game, name)?)); }
    let files = take(state, token, Instant::now())?;
    let mut report = Report { installed: Vec::new(), skipped: Vec::new() };
    for source in files {
        let targets: Vec<(Option<String>, PathBuf)> = match source.item.kind {
            Kind::Mod => vec![(None, game.join("mods"))], Kind::Resourcepack => vec![(None, game.join("resourcepacks"))], Kind::Shader => vec![(None, game.join("shaderpacks"))],
            Kind::Datapack => dirs.iter().map(|(n, p)| (Some(n.clone()), p.join("datapacks"))).collect(),
        };
        if targets.is_empty() { report.skipped.push(Skipped { name: source.item.name.clone(), world: None, reason: "choose at least one world".into() }); }
        for (world, folder) in targets { match copy_file(&source, &folder) {
            Ok(()) => report.installed.push(Outcome { name: source.item.name.clone(), world }),
            Err(reason) => report.skipped.push(Skipped { name: source.item.name.clone(), world, reason }),
        } }
    }
    Ok(report)
}
fn copy_file(source: &Source, folder: &Path) -> Result<(), String> {
    let parent = folder.parent().ok_or("invalid destination")?;
    crate::cachefile::check_directory(parent).map_err(|_| "destination is not a plain directory")?;
    match fs::create_dir(folder) { Ok(()) => (), Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (), Err(_) => return Err("cannot create destination folder".into()) }
    crate::cachefile::check_directory(folder).map_err(|_| "destination is not a plain directory")?;
    if plain_file(&source.path)?.len() != source.item.size { return Err("dropped file changed".into()); }
    let mut input = fs::File::open(&source.path).map_err(|_| "cannot open dropped file")?;
    if input.metadata().map_err(|_| "cannot read dropped file")?.len() != source.item.size { return Err("dropped file changed".into()); }
    let target = folder.join(&source.item.name);
    let temp = folder.join(format!(".aether-drop-{}-{}.part", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let mut output = fs::OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|_| "cannot write temporary file")?;
    let claim = fs::OpenOptions::new().write(true).create_new(true).open(&target);
    if claim.is_err() { let _ = fs::remove_file(&temp); return Err("a file with this name is already there".into()); }
    drop(claim);
    let result = (|| -> Result<(), String> {
        let mut buffer = [0u8; 65536]; let mut total = 0u64;
        loop { let n = input.read(&mut buffer).map_err(|_| "cannot read dropped file")?; if n == 0 { break; } total += n as u64; if total > source.item.size { return Err("dropped file changed".into()); } output.write_all(&buffer[..n]).map_err(|_| "cannot write file")?; }
        if total != source.item.size { return Err("dropped file changed".into()); }
        output.flush().map_err(|_| "cannot write file")?; drop(output);
        fs::rename(&temp, &target).map_err(|_| "cannot place file")?; Ok(())
    })();
    if result.is_err() { let _ = fs::remove_file(&temp); let _ = fs::remove_file(&target); }
    result
}
#[cfg(test)]
#[path = "drop_tests.rs"]
mod tests;
