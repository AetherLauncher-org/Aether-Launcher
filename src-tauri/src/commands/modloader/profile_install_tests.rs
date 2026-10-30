//! `ClientInstall` 的行为：不联网、不起 Java，用内存里拼出来的 installer jar、假的传输层和
//! 假的处理器运行器，验证校验、路径、处理器顺序和失败时的状态。

use std::collections::HashMap;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use aether_download::hash::HashingWriter;
use aether_download::transport::{
    HttpTransport, TransportError, TransportRequest, TransportResponse,
};

use super::*;

// ---- 小工具 ----------------------------------------------------------------------

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "aether-profile-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("temp dir");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn sha1_hex(bytes: &[u8]) -> String {
    let mut writer = HashingWriter::new(io::sink());
    writer.write_all(bytes).expect("hash");
    writer.finish().2
}

/// 拼一个 zip（jar）：`(名字, 内容)`。
fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for (name, bytes) in entries {
        writer.start_file(*name, options).expect("start file");
        writer.write_all(bytes).expect("write entry");
    }
    writer.finish().expect("finish").into_inner()
}

/// 一个带 `Main-Class` 的处理器 jar。
fn processor_jar() -> Vec<u8> {
    zip_bytes(&[(
        "META-INF/MANIFEST.MF",
        b"Manifest-Version: 1.0\r\nMain-Class: net.example.Tool\r\n\r\n",
    )])
}

struct Routes {
    routes: HashMap<String, Vec<u8>>,
    requested: Mutex<Vec<String>>,
}

impl Routes {
    fn new(routes: &[(&str, &[u8])]) -> Self {
        Self {
            routes: routes
                .iter()
                .map(|(url, body)| ((*url).to_owned(), body.to_vec()))
                .collect(),
            requested: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<String> {
        self.requested.lock().expect("lock").clone()
    }
}

impl HttpTransport for Routes {
    fn get(&self, request: &TransportRequest) -> Result<TransportResponse, TransportError> {
        self.requested
            .lock()
            .expect("lock")
            .push(request.url.clone());
        let Some(body) = self.routes.get(&request.url) else {
            return Err(TransportError::HttpStatus {
                url: request.url.clone(),
                status: 404,
            });
        };
        Ok(TransportResponse {
            status: 200,
            content_length: Some(body.len() as u64),
            supports_range: false,
            content_range: None,
            body: Box::new(Cursor::new(body.clone())),
        })
    }
}

/// 一次处理器调用：classpath、主类、参数。
type Call = (Vec<PathBuf>, String, Vec<String>);

/// 记录每次调用；每次运行把 `produce` 写到 `--out` 后面那个路径。
struct FakeRunner {
    calls: Mutex<Vec<Call>>,
    produce: Vec<u8>,
    fail: bool,
}

impl FakeRunner {
    fn new(produce: &[u8]) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            produce: produce.to_vec(),
            fail: false,
        }
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().expect("lock").clone()
    }
}

impl ProcessorRunner for FakeRunner {
    fn run(
        &self,
        classpath: &[PathBuf],
        main_class: &str,
        args: &[String],
        _cwd: &Path,
        _cancel: &CancellationToken,
    ) -> Result<(), String> {
        self.calls.lock().expect("lock").push((
            classpath.to_vec(),
            main_class.to_owned(),
            args.to_vec(),
        ));
        if self.fail {
            return Err("exit code Some(1): boom".to_owned());
        }
        if let Some(position) = args.iter().position(|arg| arg == "--out") {
            let target = PathBuf::from(&args[position + 1]);
            fs::create_dir_all(target.parent().expect("parent")).expect("create parent");
            fs::write(target, &self.produce).expect("write output");
        }
        Ok(())
    }
}

// ---- 一个完整的、合成的 installer -------------------------------------------------

const DEP_URL: &str = "https://maven.example.invalid/net/example/dep/1.0/dep-1.0.jar";
const OUTPUT: &[u8] = b"patched client";

struct Fixture {
    dir: TempDir,
    installer: PathBuf,
    transport: Routes,
    dep: Vec<u8>,
}

impl Fixture {
    /// `tweak` 改 install_profile.json 的文本；`tool` 是处理器 jar 的内容。
    fn with(tag: &str, tweak: impl Fn(String) -> String, tool: &[u8]) -> Self {
        let dir = TempDir::new(tag);
        let dep = b"dependency bytes".to_vec();
        let profile = format!(
            r#"{{
              "spec": 1, "profile": "test", "version": "9.9-test-1.0",
              "json": "/version.json", "minecraft": "1.20.1",
              "data": {{
                "PATCH": {{"client": "/data/client.lzma"}},
                "OUT": {{"client": "[net.example:out:1.0:slim]"}},
                "OUT_SHA": {{"client": "'{out_sha}'"}},
                "LIT": {{"client": "'literal value'"}}
              }},
              "processors": [
                {{"sides": ["server"], "jar": "net.example:tool:1.0", "classpath": [], "args": ["--server-only"]}},
                {{"jar": "net.example:tool:1.0", "classpath": ["net.example:dep:1.0"],
                  "args": ["--in", "{{MINECRAFT_JAR}}", "--patch", "{{PATCH}}", "--out", "{{OUT}}",
                           "--lit", "{{LIT}}", "--root", "{{ROOT}}", "--side", "{{SIDE}}"],
                  "outputs": {{"{{OUT}}": "{{OUT_SHA}}"}}}}
              ],
              "libraries": [
                {{"name": "net.example:tool:1.0", "downloads": {{"artifact": {{
                   "path": "net/example/tool/1.0/tool-1.0.jar", "url": "",
                   "sha1": "{tool_sha}", "size": {tool_size}}}}}}},
                {{"name": "net.example:dep:1.0", "downloads": {{"artifact": {{
                   "path": "net/example/dep/1.0/dep-1.0.jar", "url": "{DEP_URL}",
                   "sha1": "{dep_sha}", "size": {dep_size}}}}}}}
              ]
            }}"#,
            out_sha = sha1_hex(OUTPUT),
            tool_sha = sha1_hex(tool),
            tool_size = tool.len(),
            dep_sha = sha1_hex(&dep),
            dep_size = dep.len(),
        );
        let version = br#"{"id": "9.9-test-1.0", "inheritsFrom": "1.20.1", "libraries": []}"#;
        let installer_bytes = zip_bytes(&[
            ("install_profile.json", tweak(profile).as_bytes()),
            ("version.json", version),
            ("data/client.lzma", b"binary patch"),
            ("maven/net/example/tool/1.0/tool-1.0.jar", tool),
        ]);
        let installer = dir.0.join("installer.jar");
        fs::write(&installer, installer_bytes).expect("write installer");
        // 处理器要读 Minecraft 的 jar 路径，只是个参数，不需要真的存在。
        let transport = Routes::new(&[(DEP_URL, &dep)]);
        Self {
            dir,
            installer,
            transport,
            dep,
        }
    }

    fn standard(tag: &str) -> Self {
        Self::with(tag, |profile| profile, &processor_jar())
    }

    fn game_root(&self) -> PathBuf {
        self.dir.0.join("game")
    }

    fn install(&self, runner: &dyn ProcessorRunner) -> Result<String, String> {
        let cancel = CancellationToken::new();
        ClientInstall {
            transport: &self.transport,
            game_root: &self.game_root(),
            installer: &self.installer,
            runner,
            cancel: &cancel,
        }
        .run()
    }
}

// ---- 成功路径 ----------------------------------------------------------------------

#[test]
fn installs_libraries_runs_client_processors_and_writes_the_version() {
    let fixture = Fixture::standard("ok");
    let runner = FakeRunner::new(OUTPUT);

    let id = fixture.install(&runner).expect("a valid profile installs");

    assert_eq!(id, "9.9-test-1.0");
    let root = fixture.game_root();
    // 下载的库和 installer 自带的库都落了盘。
    assert_eq!(
        fs::read(root.join("libraries/net/example/dep/1.0/dep-1.0.jar")).expect("dep"),
        fixture.dep
    );
    assert!(
        root.join("libraries/net/example/tool/1.0/tool-1.0.jar")
            .is_file()
    );
    // 版本 JSON 最后写，在正确的位置。
    let version =
        fs::read_to_string(root.join("versions/9.9-test-1.0/9.9-test-1.0.json")).expect("json");
    assert!(version.contains("inheritsFrom"));
    // 没有 .part 或工作目录残留。
    assert!(!root.join("installers/9.9-test-1.0.work").exists());
}

#[test]
fn only_the_client_processors_run_with_expanded_arguments_and_the_right_classpath() {
    let fixture = Fixture::standard("args");
    let runner = FakeRunner::new(OUTPUT);

    fixture.install(&runner).expect("installs");

    let calls = runner.calls();
    assert_eq!(calls.len(), 1, "the server-only processor must be skipped");
    let (classpath, main_class, args) = &calls[0];
    assert_eq!(main_class, "net.example.Tool");

    let root = fixture.game_root();
    let libraries = root.join("libraries");
    assert_eq!(
        classpath,
        &vec![
            libraries.join("net/example/tool/1.0/tool-1.0.jar"),
            libraries.join("net/example/dep/1.0/dep-1.0.jar"),
        ],
        "the processor's own jar comes first, then its classpath"
    );

    let value_after = |flag: &str| {
        let position = args.iter().position(|arg| arg == flag).expect(flag);
        args[position + 1].clone()
    };
    assert_eq!(
        value_after("--in"),
        root.join("versions")
            .join("1.20.1")
            .join("1.20.1.jar")
            .to_string_lossy()
    );
    assert!(value_after("--patch").ends_with("client.lzma"));
    assert_eq!(value_after("--lit"), "literal value");
    assert_eq!(value_after("--root"), root.to_string_lossy());
    assert_eq!(value_after("--side"), "client");
    assert_eq!(
        value_after("--out"),
        libraries
            .join("net/example/out/1.0/out-1.0-slim.jar")
            .to_string_lossy()
    );
}

#[test]
fn a_processor_whose_outputs_already_match_is_not_run_again() {
    let fixture = Fixture::standard("cached");
    let first = FakeRunner::new(OUTPUT);
    fixture.install(&first).expect("first install");
    assert_eq!(first.calls().len(), 1);

    let second = FakeRunner::new(b"would be wrong");
    fixture.install(&second).expect("second install");

    assert!(second.calls().is_empty(), "verified outputs are reused");
}

#[test]
fn a_library_that_is_already_on_disk_and_matches_is_not_downloaded_again() {
    let fixture = Fixture::standard("present");
    let runner = FakeRunner::new(OUTPUT);
    fixture.install(&runner).expect("first install");
    let after_first = fixture.transport.requests().len();

    fixture
        .install(&FakeRunner::new(OUTPUT))
        .expect("second install");

    assert_eq!(fixture.transport.requests().len(), after_first);
}

// ---- 校验 --------------------------------------------------------------------------

#[test]
fn a_downloaded_library_with_the_wrong_hash_stops_the_install_and_leaves_no_version() {
    let fixture = Fixture::with(
        "badlib",
        |profile| profile.replace(&sha1_hex(b"dependency bytes"), &"0".repeat(40)),
        &processor_jar(),
    );
    let runner = FakeRunner::new(OUTPUT);

    let error = fixture.install(&runner).expect_err("a bad hash must fail");

    assert!(error.contains("dep-1.0.jar"), "{error}");
    assert!(
        runner.calls().is_empty(),
        "nothing may run after a failed check"
    );
    assert!(!fixture.game_root().join("versions").exists());
    assert!(
        !fixture
            .game_root()
            .join("libraries/net/example/dep/1.0/dep-1.0.jar")
            .exists()
    );
}

#[test]
fn a_bundled_library_with_the_wrong_hash_is_refused() {
    let tool = processor_jar();
    let fixture = Fixture::with(
        "badbundled",
        |profile| profile.replace(&sha1_hex(&processor_jar()), &"1".repeat(40)),
        &tool,
    );
    let runner = FakeRunner::new(OUTPUT);

    let error = fixture.install(&runner).expect_err("must fail");

    assert!(error.contains("tool-1.0.jar"), "{error}");
    assert!(runner.calls().is_empty());
}

#[test]
fn a_library_without_a_sha1_is_refused_not_trusted() {
    let fixture = Fixture::with(
        "nosha",
        |profile| profile.replace(&sha1_hex(b"dependency bytes"), "not-a-hash"),
        &processor_jar(),
    );
    let runner = FakeRunner::new(OUTPUT);

    let error = fixture.install(&runner).expect_err("must fail");

    assert!(error.contains("no SHA-1"), "{error}");
    assert!(runner.calls().is_empty());
}

#[test]
fn a_library_path_that_escapes_the_game_tree_is_refused() {
    let fixture = Fixture::with(
        "escape",
        |profile| profile.replace("net/example/dep/1.0/dep-1.0.jar", "../../evil.jar"),
        &processor_jar(),
    );
    let runner = FakeRunner::new(OUTPUT);

    let error = fixture.install(&runner).expect_err("must fail");

    assert!(error.contains("unsafe path"), "{error}");
    assert!(!fixture.dir.0.join("evil.jar").exists());
    assert!(
        fixture.transport.requests().is_empty(),
        "no request before the path check"
    );
}

#[test]
fn a_processor_that_produces_the_wrong_output_fails_the_install() {
    let fixture = Fixture::standard("badout");
    let runner = FakeRunner::new(b"something else");

    let error = fixture.install(&runner).expect_err("must fail");

    assert!(error.contains("expected SHA-1"), "{error}");
    assert!(
        !fixture.game_root().join("versions").exists(),
        "no version JSON after a failure"
    );
}

/// 在描述的库列表里加一个没有下载地址、installer 里也没有的库。
fn with_generated_library(path: &str, sha1: &str) -> impl Fn(String) -> String {
    let entry = format!(
        r#""libraries": [
          {{"name": "net.example:gen:1.0", "downloads": {{"artifact": {{
             "path": "{path}", "url": "", "sha1": "{sha1}", "size": 1}}}}}},"#
    );
    move |profile: String| profile.replace(r#""libraries": ["#, &entry)
}

#[test]
fn a_library_that_a_processor_produces_is_not_downloaded_up_front() {
    // Forge 26.x 的版本 JSON 把打过补丁的 `forge-…-client.jar` 列成一个 url 为空的库。
    let fixture = Fixture::with(
        "generated",
        with_generated_library("net/example/out/1.0/out-1.0-slim.jar", &sha1_hex(OUTPUT)),
        &processor_jar(),
    );
    let runner = FakeRunner::new(OUTPUT);

    fixture
        .install(&runner)
        .expect("a processor-made library is accepted once it exists");
    assert!(
        fixture
            .game_root()
            .join("libraries/net/example/out/1.0/out-1.0-slim.jar")
            .is_file()
    );
}

#[test]
fn a_library_nobody_provides_still_fails_the_install() {
    let fixture = Fixture::with(
        "ungenerated",
        with_generated_library("net/example/ghost/1.0/ghost-1.0.jar", &sha1_hex(b"ghost")),
        &processor_jar(),
    );
    let runner = FakeRunner::new(OUTPUT);

    let error = fixture.install(&runner).expect_err("must fail");

    assert!(error.contains("no processor produced it"), "{error}");
    assert!(!fixture.game_root().join("versions").exists());
}

#[test]
fn a_failing_processor_reports_the_error_and_writes_no_version() {
    let fixture = Fixture::standard("procfail");
    let mut runner = FakeRunner::new(OUTPUT);
    runner.fail = true;

    let error = fixture.install(&runner).expect_err("must fail");

    assert!(error.contains("boom"), "{error}");
    assert!(!fixture.game_root().join("versions").exists());
}

#[test]
fn a_processor_jar_without_a_main_class_is_refused() {
    let no_main = zip_bytes(&[("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0\r\n\r\n")]);
    let fixture = Fixture::with("nomain", |profile| profile, &no_main);
    let runner = FakeRunner::new(OUTPUT);

    let error = fixture.install(&runner).expect_err("must fail");

    assert!(error.contains("Main-Class"), "{error}");
}

#[test]
fn the_old_installer_format_is_reported_clearly() {
    let dir = TempDir::new("old");
    let installer = dir.0.join("installer.jar");
    let profile =
        br#"{"install": {"path": "net.minecraftforge:forge:1.7.10"}, "versionInfo": {"id": "x"}}"#;
    fs::write(&installer, zip_bytes(&[("install_profile.json", profile)])).expect("write");
    let transport = Routes::new(&[]);
    let cancel = CancellationToken::new();

    let error = ClientInstall {
        transport: &transport,
        game_root: &dir.0.join("game"),
        installer: &installer,
        runner: &FakeRunner::new(b""),
        cancel: &cancel,
    }
    .run()
    .expect_err("the old format is not supported");

    assert!(error.contains("old installer format"), "{error}");
}

#[test]
fn a_cancelled_install_stops_before_downloading() {
    let fixture = Fixture::standard("cancel");
    let cancel = CancellationToken::new();
    cancel.cancel();

    let error = ClientInstall {
        transport: &fixture.transport,
        game_root: &fixture.game_root(),
        installer: &fixture.installer,
        runner: &FakeRunner::new(OUTPUT),
        cancel: &cancel,
    }
    .run()
    .expect_err("must stop");

    assert!(error.contains("cancelled"), "{error}");
    assert!(fixture.transport.requests().is_empty());
}

// ---- 表达式和坐标 ------------------------------------------------------------------

#[test]
fn maven_coordinates_become_library_paths() {
    assert_eq!(
        artifact_path("net.minecraftforge:forge:1.20.1-47.4.0").as_deref(),
        Some("net/minecraftforge/forge/1.20.1-47.4.0/forge-1.20.1-47.4.0.jar")
    );
    assert_eq!(
        artifact_path("net.minecraft:client:1.20.1-2023:slim").as_deref(),
        Some("net/minecraft/client/1.20.1-2023/client-1.20.1-2023-slim.jar")
    );
    assert_eq!(
        artifact_path("de.oceanlabs.mcp:mcp_config:1.20.1-2023@zip").as_deref(),
        Some("de/oceanlabs/mcp/mcp_config/1.20.1-2023/mcp_config-1.20.1-2023.zip")
    );
    assert_eq!(
        artifact_path("net.minecraft:client:1.20.1-2023:mappings@txt").as_deref(),
        Some("net/minecraft/client/1.20.1-2023/client-1.20.1-2023-mappings.txt")
    );
}

#[test]
fn malformed_or_hostile_coordinates_are_rejected() {
    for bad in [
        "",
        "a:b",
        "a:b:c:d:e",
        "a::1.0",
        ":b:1.0",
        "a:b:../../x",
        "a:../b:1.0",
        "a:b:1.0@",
        "a:b:1.0@../x",
        "a/b:c:1.0",
        "a:b:1.0:cl\\x",
    ] {
        assert!(artifact_path(bad).is_none(), "should be rejected: {bad:?}");
    }
}

#[test]
fn expressions_expand_artifacts_literals_and_variables() {
    let libraries = Path::new("/game/libraries");
    let variables: BTreeMap<String, String> = [
        ("ROOT".to_owned(), "/game".to_owned()),
        ("SIDE".to_owned(), "client".to_owned()),
    ]
    .into();

    assert_eq!(
        expand("[a.b:c:1.0]", &variables, libraries).expect("artifact"),
        Path::new("/game/libraries")
            .join("a/b/c/1.0/c-1.0.jar")
            .to_string_lossy()
    );
    assert_eq!(
        expand("'quoted'", &variables, libraries).expect("literal"),
        "quoted"
    );
    assert_eq!(
        expand("{ROOT}/run-{SIDE}.sh", &variables, libraries).expect("variables"),
        "/game/run-client.sh"
    );
    assert_eq!(
        expand("plain", &variables, libraries).expect("plain"),
        "plain"
    );
}

#[test]
fn an_unknown_or_unterminated_variable_is_an_error_not_passed_through() {
    let variables = BTreeMap::new();
    let libraries = Path::new("/game/libraries");

    assert!(expand("{MISSING}", &variables, libraries).is_err());
    assert!(expand("{ROOT", &variables, libraries).is_err());
    assert!(expand("[not a coordinate]", &variables, libraries).is_err());
}

#[test]
fn the_main_class_is_read_across_manifest_line_wraps() {
    let manifest = "Manifest-Version: 1.0\r\nMain-Class: net.example.very.long.package.name.that.wraps\r\n .onto.a.second.line.Main\r\nOther: x\r\n";
    assert_eq!(
        main_class_of(manifest).as_deref(),
        Some("net.example.very.long.package.name.that.wraps.onto.a.second.line.Main")
    );
    assert_eq!(main_class_of("Manifest-Version: 1.0\r\n"), None);
}
