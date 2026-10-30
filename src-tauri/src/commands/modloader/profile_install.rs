//! Forge / NeoForge 的客户端安装：自己做官方 installer 做的事。
//!
//! 以前的做法是 `java -jar installer.jar --installClient`：把一个下载来的程序整个交出去
//! 执行，它自己下载什么、写什么、改哪个文件，启动器都管不到，还要求目录里已经有官方
//! 启动器写的 `launcher_profiles.json`。这里改成**读 installer 里的安装描述，自己执行**：
//!
//! 1. 读 `install_profile.json` 和它指向的版本 JSON（都在 installer jar 里；installer 本身
//!    已经按官方 maven 的 `.sha1` 校验过）；
//! 2. 描述里列的每个库都带 SHA-1：下载并逐个校验，installer 自带的（`url` 为空）从 jar
//!    里取出来校验；
//! 3. 按描述里的顺序运行「处理器」（对 Minecraft 客户端打补丁的 Java 程序）。处理器的 jar
//!    和它的 classpath 都是第 2 步校验过的库；它的输出文件如果描述里给了 SHA-1，运行后
//!    也要核对；
//! 4. 最后写版本 JSON。
//!
//! 和官方 installer 的区别：每个文件都有校验值才会被使用或执行；不碰 `launcher_profiles.json`；
//! 路径都经过 `is_safe_relative_path`；进度和取消由启动器控制。
//!
//! 老格式（`install` + `versionInfo`，约 2017 年之前的 Forge）不支持，会明确报错，而不是
//! 悄悄装坏。

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use aether_download::CancellationToken;
use aether_download::hash::HashingWriter;
use aether_download::is_safe_relative_path;
use aether_download::transport::HttpTransport;
use serde_json::Value;

use super::fetch::download_into;

/// `install_profile.json` 和版本 JSON 的大小上限。真实的在几十到几百 KB。
const MAX_JSON_BYTES: u64 = 8 * 1024 * 1024;

/// 从 installer jar 里取出的单个文件的大小上限（补丁、库、清单都远小于它）。
const MAX_ENTRY_BYTES: u64 = 512 * 1024 * 1024;

/// 运行一个处理器。生产实现起 Java 进程；测试里用假的，不需要 Java。
pub(super) trait ProcessorRunner {
    /// 用 `classpath` 运行 `main_class`，参数是 `args`，工作目录是 `cwd`。
    ///
    /// 实现要在 `cancel` 被置位时尽快停下并返回错误。
    fn run(
        &self,
        classpath: &[PathBuf],
        main_class: &str,
        args: &[String],
        cwd: &Path,
        cancel: &CancellationToken,
    ) -> Result<(), String>;
}

/// 一个处理器：Java 程序 + 它的 classpath + 参数 + 期望的输出。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Processor {
    /// 只在这些端运行；`None` 表示所有端。
    sides: Option<Vec<String>>,
    jar: String,
    classpath: Vec<String>,
    args: Vec<String>,
    /// 输出文件表达式 → 期望的 SHA-1 表达式。
    outputs: BTreeMap<String, String>,
}

/// 描述里列的一个库。`sha1` 必须有。
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProfileLibrary {
    path: String,
    url: Option<String>,
    sha1: String,
    size: Option<u64>,
}

/// 解析后的现代格式 `install_profile.json`。
#[derive(Debug)]
struct Profile {
    minecraft: String,
    json: String,
    data: BTreeMap<String, String>,
    processors: Vec<Processor>,
    libraries: Vec<ProfileLibrary>,
}

fn is_sha1(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// `group:artifact:version[:classifier][@ext]` → 相对 `libraries/` 的路径。
///
/// 非法坐标（段数不对、空段、带路径分隔符或 `..`）返回 `None`。
pub(super) fn artifact_path(coordinate: &str) -> Option<String> {
    let (coordinate, extension) = match coordinate.split_once('@') {
        Some((coordinate, extension)) => (coordinate, extension),
        None => (coordinate, "jar"),
    };
    let parts: Vec<&str> = coordinate.split(':').collect();
    if !(3..=4).contains(&parts.len()) || extension.is_empty() {
        return None;
    }
    if parts.iter().chain([&extension]).any(|part| {
        part.is_empty()
            || part.contains(['/', '\\'])
            || part.contains("..")
            || part.contains(['@', ' '])
    }) {
        return None;
    }
    let (group, artifact, version) = (parts[0], parts[1], parts[2]);
    let classifier = parts
        .get(3)
        .map(|value| format!("-{value}"))
        .unwrap_or_default();
    let path = format!(
        "{}/{artifact}/{version}/{artifact}-{version}{classifier}.{extension}",
        group.replace('.', "/")
    );
    is_safe_relative_path(Path::new(&path)).then_some(path)
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// 读一个库列表。每个库必须有 `downloads.artifact.path` 和 SHA-1：没有校验值的库不会被使用。
fn parse_libraries(list: Option<&Value>) -> Result<Vec<ProfileLibrary>, String> {
    let Some(list) = list.and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut libraries = Vec::new();
    for entry in list {
        let name = string_field(entry, "name").unwrap_or_else(|| "<unnamed>".to_owned());
        // 没有 artifact 的条目（纯 natives 声明之类）没有要下载的东西。
        let Some(artifact) = entry
            .get("downloads")
            .and_then(|downloads| downloads.get("artifact"))
        else {
            continue;
        };
        let path =
            string_field(artifact, "path").ok_or_else(|| format!("library {name} has no path"))?;
        if !is_safe_relative_path(Path::new(&path)) {
            return Err(format!("library {name} has an unsafe path: {path}"));
        }
        let sha1 = string_field(artifact, "sha1")
            .filter(|sha1| is_sha1(sha1))
            .ok_or_else(|| {
                format!("library {name} has no SHA-1; refusing to use a file with no checksum")
            })?
            .to_ascii_lowercase();
        let url = string_field(artifact, "url").filter(|url| !url.trim().is_empty());
        libraries.push(ProfileLibrary {
            path,
            url,
            sha1,
            size: artifact.get("size").and_then(Value::as_u64),
        });
    }
    Ok(libraries)
}

/// 解析 `install_profile.json`。老格式给出明确的错误。
fn parse_profile(text: &str) -> Result<Profile, String> {
    let value: Value = serde_json::from_str(text)
        .map_err(|error| format!("install_profile.json is not valid JSON: {error}"))?;

    if value.get("versionInfo").is_some() && value.get("processors").is_none() {
        return Err(
            "this Forge version uses the old installer format (before 2017), which is not supported"
                .to_owned(),
        );
    }

    let minecraft = string_field(&value, "minecraft")
        .ok_or_else(|| "install_profile.json does not name a Minecraft version".to_owned())?;
    let json = string_field(&value, "json")
        .ok_or_else(|| "install_profile.json does not point at a version JSON".to_owned())?;

    // data：每个键在 client 端的取值。
    let mut data = BTreeMap::new();
    if let Some(entries) = value.get("data").and_then(Value::as_object) {
        for (key, sides) in entries {
            if let Some(client) = string_field(sides, "client") {
                data.insert(key.clone(), client);
            }
        }
    }

    let mut processors = Vec::new();
    for entry in value
        .get("processors")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let jar = string_field(entry, "jar").ok_or_else(|| "a processor has no jar".to_owned())?;
        let outputs = entry
            .get("outputs")
            .and_then(Value::as_object)
            .map(|map| {
                map.iter()
                    .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_owned())))
                    .collect()
            })
            .unwrap_or_default();
        processors.push(Processor {
            sides: entry.get("sides").map(|sides| string_list(Some(sides))),
            jar,
            classpath: string_list(entry.get("classpath")),
            args: string_list(entry.get("args")),
            outputs,
        });
    }

    Ok(Profile {
        minecraft,
        json,
        data,
        processors,
        libraries: parse_libraries(value.get("libraries"))?,
    })
}

/// 展开一个表达式：`[坐标]` → 库文件的绝对路径，`'字面量'` → 去掉引号，
/// 其余把 `{变量}` 换成值。出现没定义的变量是错误，不是留着原样。
fn expand(
    value: &str,
    variables: &BTreeMap<String, String>,
    libraries: &Path,
) -> Result<String, String> {
    if value.len() >= 2 && value.starts_with('[') && value.ends_with(']') {
        let coordinate = &value[1..value.len() - 1];
        let path = artifact_path(coordinate)
            .ok_or_else(|| format!("not a valid Maven coordinate: {coordinate}"))?;
        return Ok(libraries.join(path).to_string_lossy().into_owned());
    }
    if value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'') {
        return Ok(value[1..value.len() - 1].to_owned());
    }

    let mut result = String::new();
    let mut rest = value;
    while let Some(open) = rest.find('{') {
        result.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            return Err(format!("unterminated variable in {value}"));
        };
        let name = &after[..close];
        let replacement = variables
            .get(name)
            .ok_or_else(|| format!("unknown variable {{{name}}} in {value}"))?;
        result.push_str(replacement);
        rest = &after[close + 1..];
    }
    result.push_str(rest);
    Ok(result)
}

/// `META-INF/MANIFEST.MF` 里的 `Main-Class`（处理续行：以一个空格开头的行接在上一行后面）。
fn main_class_of(manifest: &str) -> Option<String> {
    let mut joined: Vec<String> = Vec::new();
    for line in manifest.lines() {
        if let Some(continuation) = line.strip_prefix(' ') {
            if let Some(last) = joined.last_mut() {
                last.push_str(continuation);
            }
        } else {
            joined.push(line.to_owned());
        }
    }
    joined.iter().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("Main-Class")
            .then(|| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    })
}

/// 读 `reader` 到末尾，返回（字节数，SHA-1），最多读 `limit` 字节，超了就报错。
fn hash_reader(reader: &mut dyn Read, limit: u64) -> Result<(u64, String), String> {
    let mut writer = HashingWriter::new(io::sink());
    let copied = io::copy(&mut reader.take(limit.saturating_add(1)), &mut writer)
        .map_err(|error| format!("cannot read: {error}"))?;
    if copied > limit {
        return Err(format!("larger than the {limit} byte limit"));
    }
    let (_, size, sha1) = writer.finish();
    Ok((size, sha1))
}

/// 文件存在且 SHA-1 匹配。
fn file_matches(path: &Path, sha1: &str) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    hash_reader(&mut file, MAX_ENTRY_BYTES)
        .is_ok_and(|(_, actual)| actual.eq_ignore_ascii_case(sha1))
}

/// 把 `reader` 写到 `target`：先写 `.part`，大小和 SHA-1 通过后才改名到位。
fn write_verified(
    target: &Path,
    reader: &mut dyn Read,
    expected_sha1: &str,
    expected_size: Option<u64>,
) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let mut part_name = target
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    part_name.push(".part");
    let part = target.with_file_name(part_name);

    let outcome = (|| {
        let file = fs::File::create(&part)
            .map_err(|error| format!("cannot create {}: {error}", part.display()))?;
        let mut writer = HashingWriter::new(file);
        let copied = io::copy(&mut reader.take(MAX_ENTRY_BYTES + 1), &mut writer)
            .map_err(|error| format!("cannot write {}: {error}", part.display()))?;
        let (mut file, size, sha1) = writer.finish();
        file.flush()
            .map_err(|error| format!("cannot write {}: {error}", part.display()))?;
        drop(file);
        if copied > MAX_ENTRY_BYTES {
            return Err("file is larger than the size limit".to_owned());
        }
        if expected_size.is_some_and(|expected| size != expected) {
            return Err(format!(
                "file is {size} bytes, not the {} the profile lists",
                expected_size.unwrap_or_default()
            ));
        }
        if !sha1.eq_ignore_ascii_case(expected_sha1) {
            return Err(format!(
                "SHA-1 is {sha1}, but the profile says {expected_sha1}"
            ));
        }
        fs::rename(&part, target)
            .map_err(|error| format!("cannot move the verified file into place: {error}"))
    })();
    if outcome.is_err() {
        let _ = fs::remove_file(&part);
    }
    outcome
}

/// 先写 `.part` 再改名：中断不会留下写了一半的文件。
fn write_bytes_atomically(target: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let mut part_name = target
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    part_name.push(".part");
    let part = target.with_file_name(part_name);
    let outcome = fs::write(&part, bytes)
        .and_then(|()| fs::rename(&part, target))
        .map_err(|error| format!("cannot write {}: {error}", target.display()));
    if outcome.is_err() {
        let _ = fs::remove_file(&part);
    }
    outcome
}

/// 读 jar 里的一个条目到内存，有大小上限。
fn read_entry<R: Read + io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<Vec<u8>, String> {
    let entry = archive
        .by_name(name)
        .map_err(|_| format!("the installer has no {name}"))?;
    let mut bytes = Vec::new();
    entry
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read {name} from the installer: {error}"))?;
    if bytes.len() as u64 > limit {
        return Err(format!(
            "{name} in the installer is larger than the {limit} byte limit"
        ));
    }
    Ok(bytes)
}

/// 做完一个客户端安装需要的全部输入。
pub(super) struct ClientInstall<'a> {
    pub transport: &'a dyn HttpTransport,
    /// 游戏树根目录（`<app data>/game`）。
    pub game_root: &'a Path,
    /// 已经校验过的 installer jar。
    pub installer: &'a Path,
    pub runner: &'a dyn ProcessorRunner,
    pub cancel: &'a CancellationToken,
}

impl ClientInstall<'_> {
    fn libraries_dir(&self) -> PathBuf {
        self.game_root.join("libraries")
    }

    fn check_cancelled(&self) -> Result<(), String> {
        if self.cancel.is_cancelled() {
            return Err("installation cancelled".to_owned());
        }
        Ok(())
    }

    /// 执行安装，返回写下的版本 id。
    pub(super) fn run(&self) -> Result<String, String> {
        let file = fs::File::open(self.installer)
            .map_err(|error| format!("cannot open the installer: {error}"))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|error| format!("the installer is not a valid jar: {error}"))?;

        let profile_bytes = read_entry(&mut archive, "install_profile.json", MAX_JSON_BYTES)?;
        let profile = parse_profile(&String::from_utf8_lossy(&profile_bytes))?;

        let version_name = profile.json.trim_start_matches('/').to_owned();
        let version_bytes = read_entry(&mut archive, &version_name, MAX_JSON_BYTES)?;
        let version: Value = serde_json::from_slice(&version_bytes)
            .map_err(|error| format!("{version_name} is not valid JSON: {error}"))?;
        let version_id = string_field(&version, "id")
            .filter(|id| is_safe_relative_path(Path::new(id)) && !id.contains(['/', '\\']))
            .ok_or_else(|| format!("{version_name} has no usable id"))?;

        // 1. 库：描述里的和版本 JSON 里的，去重后逐个下载并校验。
        let mut libraries = profile.libraries.clone();
        libraries.extend(parse_libraries(version.get("libraries"))?);
        libraries.sort_by(|a, b| a.path.cmp(&b.path));
        libraries.dedup_by(|a, b| a.path == b.path);
        // 没有下载地址、installer 里也没有的库，是处理器的产物（例如打过补丁的
        // `forge-…-client.jar`）：先不管，处理器跑完后再按描述里的 SHA-1 检查它在不在。
        let mut generated: Vec<&ProfileLibrary> = Vec::new();
        for library in &libraries {
            self.check_cancelled()?;
            if library.url.is_none()
                && !file_matches(&self.libraries_dir().join(&library.path), &library.sha1)
                && archive.by_name(&format!("maven/{}", library.path)).is_err()
            {
                generated.push(library);
                continue;
            }
            self.ensure_library(&mut archive, library)?;
        }

        // 2. 处理器要用的变量。
        let work = self
            .game_root
            .join("installers")
            .join(format!("{version_id}.work"));
        let outcome = self.run_processors(&mut archive, &profile, &work);
        let _ = fs::remove_dir_all(&work);
        outcome?;
        for library in generated {
            if !file_matches(&self.libraries_dir().join(&library.path), &library.sha1) {
                return Err(format!(
                    "{} has no download URL, is not in the installer, and no processor produced it",
                    library.path
                ));
            }
        }

        // 3. 最后写版本 JSON：它在，就说明前面都成功了。
        let relative = format!("versions/{version_id}/{version_id}.json");
        write_bytes_atomically(&self.game_root.join(&relative), &version_bytes)?;
        Ok(version_id)
    }

    fn ensure_library<R: Read + io::Seek>(
        &self,
        archive: &mut zip::ZipArchive<R>,
        library: &ProfileLibrary,
    ) -> Result<(), String> {
        let target = self.libraries_dir().join(&library.path);
        if file_matches(&target, &library.sha1) {
            return Ok(());
        }
        match &library.url {
            Some(url) => download_into(
                self.transport,
                self.game_root,
                url,
                &format!("libraries/{}", library.path),
                Some(&library.sha1),
                library.size,
            )
            .map(|_| ())
            .map_err(|error| format!("{}: {error}", library.path)),
            None => {
                // installer 自带：放在 jar 的 `maven/<path>`。
                let entry_name = format!("maven/{}", library.path);
                let mut entry = archive.by_name(&entry_name).map_err(|_| {
                    format!(
                        "{} has no download URL and is not in the installer",
                        library.path
                    )
                })?;
                write_verified(&target, &mut entry, &library.sha1, library.size)
                    .map_err(|error| format!("{}: {error}", library.path))
            }
        }
    }

    fn run_processors<R: Read + io::Seek>(
        &self,
        archive: &mut zip::ZipArchive<R>,
        profile: &Profile,
        work: &Path,
    ) -> Result<(), String> {
        let libraries_dir = self.libraries_dir();
        let minecraft_jar = self
            .game_root
            .join("versions")
            .join(&profile.minecraft)
            .join(format!("{}.jar", profile.minecraft));

        let mut variables: BTreeMap<String, String> = BTreeMap::new();
        let text = |path: &Path| path.to_string_lossy().into_owned();
        variables.insert("SIDE".to_owned(), "client".to_owned());
        variables.insert("ROOT".to_owned(), text(self.game_root));
        variables.insert("INSTALLER".to_owned(), text(self.installer));
        variables.insert("MINECRAFT_JAR".to_owned(), text(&minecraft_jar));
        variables.insert("MINECRAFT_VERSION".to_owned(), profile.minecraft.clone());
        variables.insert("LIBRARY_DIR".to_owned(), text(&libraries_dir));

        // data：`[坐标]`、`'字面量'` 或 installer 里的文件（`/data/client.lzma`，先取出来）。
        for (key, value) in &profile.data {
            let expanded = if let Some(inner) = value.strip_prefix('/') {
                if !is_safe_relative_path(Path::new(inner)) {
                    return Err(format!("data {key} points at an unsafe path: {value}"));
                }
                let bytes = read_entry(archive, inner, MAX_ENTRY_BYTES)?;
                let target = work.join(inner);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
                }
                fs::write(&target, bytes)
                    .map_err(|error| format!("cannot write {}: {error}", target.display()))?;
                text(&target)
            } else {
                expand(value, &variables, &libraries_dir)?
            };
            variables.insert(key.clone(), expanded);
        }

        for processor in &profile.processors {
            self.check_cancelled()?;
            if processor
                .sides
                .as_ref()
                .is_some_and(|sides| !sides.iter().any(|side| side == "client"))
            {
                continue;
            }

            let outputs: Vec<(PathBuf, String)> = processor
                .outputs
                .iter()
                .map(|(path, sha1)| {
                    Ok::<_, String>((
                        PathBuf::from(expand(path, &variables, &libraries_dir)?),
                        expand(sha1, &variables, &libraries_dir)?.to_ascii_lowercase(),
                    ))
                })
                .collect::<Result<_, _>>()?;
            // 输出都已经在、而且 SHA-1 对，就不用再跑（重试和重装时省掉最慢的几步）。
            if !outputs.is_empty() && outputs.iter().all(|(path, sha1)| file_matches(path, sha1)) {
                continue;
            }

            let jar = artifact_path(&processor.jar)
                .ok_or_else(|| format!("not a valid Maven coordinate: {}", processor.jar))?;
            let jar_path = libraries_dir.join(&jar);
            let mut classpath = vec![jar_path.clone()];
            for coordinate in &processor.classpath {
                let path = artifact_path(coordinate)
                    .ok_or_else(|| format!("not a valid Maven coordinate: {coordinate}"))?;
                classpath.push(libraries_dir.join(path));
            }
            if let Some(missing) = classpath.iter().find(|path| !path.is_file()) {
                return Err(format!(
                    "a processor needs {}, which the profile did not provide",
                    missing.display()
                ));
            }

            let main_class = {
                let jar_file = fs::File::open(&jar_path)
                    .map_err(|error| format!("cannot open {}: {error}", jar_path.display()))?;
                let mut jar_archive = zip::ZipArchive::new(jar_file).map_err(|error| {
                    format!("{} is not a valid jar: {error}", jar_path.display())
                })?;
                let manifest =
                    read_entry(&mut jar_archive, "META-INF/MANIFEST.MF", MAX_JSON_BYTES)?;
                main_class_of(&String::from_utf8_lossy(&manifest))
                    .ok_or_else(|| format!("{jar} has no Main-Class"))?
            };

            let args: Vec<String> = processor
                .args
                .iter()
                .map(|arg| expand(arg, &variables, &libraries_dir))
                .collect::<Result<_, _>>()?;
            self.runner
                .run(&classpath, &main_class, &args, self.game_root, self.cancel)
                .map_err(|error| format!("processor {} failed: {error}", processor.jar))?;

            for (path, sha1) in &outputs {
                if !file_matches(path, sha1) {
                    return Err(format!(
                        "processor {} did not produce {} with the expected SHA-1",
                        processor.jar,
                        path.display()
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "profile_install_tests.rs"]
mod tests;
