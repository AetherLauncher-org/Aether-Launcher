use super::*;
use std::cell::RefCell;

struct TestHost(PathBuf);
impl TestHost {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "aether-authlib-{}",
            aether_auth::AccountId::new().as_uuid()
        )))
    }
}
impl Drop for TestHost {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
impl Host for TestHost {
    fn data_dir(&self) -> Result<PathBuf, String> {
        Ok(self.0.clone())
    }
    fn is_instance_running(&self, _: aether_core::InstanceId) -> Result<bool, String> {
        Ok(false)
    }
    fn emit_login(&self, _: &crate::microsoft::LoginEvent) {}
    fn emit_install(&self, _: &crate::install::InstallProgress) {}
    fn emit_import_item(&self, _: &crate::import::ImportItemEvent) {}
    fn pick_folder(&self, _: &str) -> Result<Option<PathBuf>, String> {
        Ok(None)
    }
    fn pick_file(&self, _: &str, _: &[&str]) -> Result<Option<PathBuf>, String> {
        Ok(None)
    }
    fn known_folders(&self) -> crate::host::KnownFolders {
        crate::host::KnownFolders {
            desktop: None,
            downloads: None,
            documents: None,
            home: None,
        }
    }
}
struct Metadata(Option<String>);
impl HttpTransport for Metadata {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        assert_eq!(request.url, format!("{MANIFEST_ROOT}latest.json"));
        self.0
            .as_ref()
            .map(|s| HttpResponse::new(200, s.clone()))
            .ok_or_else(|| HttpError::transport("offline"))
    }
}
fn metadata(bytes: &[u8]) -> Metadata {
    Metadata(Some(serde_json::json!({"build_number":56,"version":"1.2.8","checksums":{"sha256":sha256_hex(bytes)},"download_url":"https://untrusted.invalid/evil.jar"}).to_string()))
}
#[test]
fn authlib_falls_back_and_uses_only_constructed_urls() {
    let host = TestHost::new();
    let calls = RefCell::new(Vec::new());
    let path = ensure_agent(&host, &metadata(b"jar"), &|url, cap| {
        assert_eq!(cap, MAX_JAR_BYTES);
        calls.borrow_mut().push(url.to_owned());
        if calls.borrow().len() == 3 {
            Ok(b"jar".to_vec())
        } else {
            Ok(b"bad".to_vec())
        }
    })
    .unwrap();
    assert_eq!(*calls.borrow(), build_urls(56, "1.2.8"));
    assert!(verify_jar(&path, &sha256_hex(b"jar")));
    assert_eq!(path.file_name().unwrap(), "authlib-injector-1.2.8.jar");
}
#[test]
fn authlib_offline_cache_is_validated_and_rehashed_each_launch() {
    let host = TestHost::new();
    let path = ensure_agent(&host, &metadata(b"jar"), &|_, _| Ok(b"jar".to_vec())).unwrap();
    assert_eq!(
        ensure_agent(&host, &Metadata(None), &|_, _| panic!("offline fetch")).unwrap(),
        path
    );
    fs::write(&path, b"tampered").unwrap();
    assert_eq!(
        ensure_agent(&host, &Metadata(None), &|_, _| panic!("offline fetch")).unwrap_err(),
        "authlib-cache-invalid"
    );
}
#[test]
fn authlib_never_overwrites_existing_version() {
    let host = TestHost::new();
    let path = ensure_agent(&host, &metadata(b"original"), &|_, _| {
        Ok(b"original".to_vec())
    })
    .unwrap();
    assert_eq!(
        ensure_agent(&host, &metadata(b"new"), &|_, _| panic!("must not fetch")).unwrap_err(),
        "authlib-cache-conflict"
    );
    assert_eq!(fs::read(path).unwrap(), b"original");
}
#[test]
fn authlib_reuses_freshly_verified_cache_without_download() {
    let host = TestHost::new();
    let first = ensure_agent(&host, &metadata(b"jar"), &|_, _| Ok(b"jar".to_vec())).unwrap();
    let second = ensure_agent(&host, &metadata(b"jar"), &|_, _| panic!("must not fetch")).unwrap();
    assert_eq!(first, second);
}
#[test]
fn authlib_rejects_oversize_even_if_hash_matches_and_fetch_ignores_cap() {
    let host = TestHost::new();
    let bytes = vec![42; MAX_JAR_BYTES as usize + 1];
    assert_eq!(
        ensure_agent(&host, &metadata(&bytes), &|_, _| Ok(bytes.clone())).unwrap_err(),
        "authlib-download-error"
    );
    assert!(!agent_root(&host).unwrap().join("verified.json").exists());
}
#[test]
fn authlib_malformed_metadata_never_falls_back_to_old_cache() {
    let host = TestHost::new();
    ensure_agent(&host, &metadata(b"jar"), &|_, _| Ok(b"jar".to_vec())).unwrap();
    assert!(
        ensure_agent(&host, &Metadata(Some("{}".to_owned())), &|_, _| panic!(
            "bad metadata"
        ))
        .is_err()
    );
    assert_eq!(
        ensure_agent(
            &host,
            &Metadata(Some(" ".repeat(MAX_MANIFEST_BYTES as usize + 1))),
            &|_, _| panic!("bad metadata")
        )
        .unwrap_err(),
        "authlib-too-large"
    );
}
#[test]
fn authlib_offline_index_cannot_supply_paths_or_invalid_fields() {
    let host = TestHost::new();
    let root = agent_root(&host).unwrap();
    prepare_root(&root).unwrap();
    for version in ["../../outside", "2", "1.2/evil"] {
        fs::write(
            root.join("verified.json"),
            serde_json::json!({"build_number":56,"version":version,"sha256":sha256_hex(b"jar")})
                .to_string(),
        )
        .unwrap();
        assert!(ensure_agent(&host, &Metadata(None), &|_, _| panic!("offline fetch")).is_err());
    }
    fs::write(
        root.join("verified.json"),
        "x".repeat(MAX_MANIFEST_BYTES as usize + 1),
    )
    .unwrap();
    assert!(ensure_agent(&host, &Metadata(None), &|_, _| panic!("offline fetch")).is_err());
}
