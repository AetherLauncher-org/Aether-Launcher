//! Read-only official Mojang catalog lookup. Never accept an IPC-supplied URL.

use std::io::Read;

use aether_download::transport::http::{KernelHttpTransport, UreqKernel};
use aether_download::transport::{HttpTransport, TransportRequest};
use aether_manifest::catalog::{
    MAX_CATALOG_BYTES, VERSION_CATALOG_URL, VersionCatalog, parse_version_catalog,
};

const CATALOG_TIMEOUT_SECONDS: u64 = 12;

/// Fetch a bounded catalog using an injectable transport for offline tests.
pub(super) fn fetch_catalog(transport: &dyn HttpTransport) -> Result<VersionCatalog, String> {
    let response = transport
        .get(&TransportRequest {
            url: VERSION_CATALOG_URL.to_owned(),
            range: None,
            timeout_secs: CATALOG_TIMEOUT_SECONDS,
        })
        .map_err(|error| format!("cannot retrieve Mojang version catalog: {error}"))?;

    let mut bytes = Vec::new();
    response
        .body
        .take((MAX_CATALOG_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read Mojang version catalog".to_owned())?;
    parse_version_catalog(&bytes)
        .map_err(|error| format!("invalid Mojang version catalog: {error}"))
}

/// No user-provided URL, file path or credentials; blocking network I/O stays off the UI thread.
#[tauri::command]
pub async fn list_minecraft_versions() -> Result<VersionCatalog, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let transport =
            KernelHttpTransport::new(UreqKernel::new()).with_max_bytes(MAX_CATALOG_BYTES as u64);
        fetch_catalog(&transport)
    })
    .await
    .map_err(|_| "Minecraft catalog lookup task failed".to_owned())?
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::Mutex;

    use aether_download::transport::{TransportError, TransportRequest, TransportResponse};

    use super::*;

    struct FakeTransport {
        data: Vec<u8>,
        urls: Mutex<Vec<String>>,
    }

    impl HttpTransport for FakeTransport {
        fn get(&self, request: &TransportRequest) -> Result<TransportResponse, TransportError> {
            self.urls.lock().unwrap().push(request.url.clone());
            Ok(TransportResponse {
                status: 200,
                content_length: Some(self.data.len() as u64),
                supports_range: false,
                content_range: None,
                body: Box::new(Cursor::new(self.data.clone())),
            })
        }
    }

    fn fake(data: Vec<u8>) -> FakeTransport {
        FakeTransport {
            data,
            urls: Mutex::new(Vec::new()),
        }
    }

    #[test]
    fn uses_only_the_fixed_mojang_endpoint() {
        let valid = format!(
            r#"{{"latest":{{"release":"1.21.1","snapshot":"24w14a"}},
                "versions":[
                {{"id":"1.21.1","type":"release","url":"https://piston-meta.mojang.com/a",
                  "sha1":"{digest}","releaseTime":"2024-08-08"}},
                {{"id":"24w14a","type":"snapshot","url":"https://piston-meta.mojang.com/b",
                  "sha1":"{digest}","releaseTime":"2024-04-01"}}]}}"#,
            digest = "0123456789abcdef0123456789abcdef01234567"
        );
        let transport = fake(valid.into_bytes());
        let catalog = fetch_catalog(&transport).unwrap();
        assert_eq!(catalog.latest.release, "1.21.1");
        let urls = transport.urls.lock().unwrap();
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0], VERSION_CATALOG_URL);
    }

    #[test]
    fn rejects_malformed_and_oversized_network_bodies() {
        assert!(fetch_catalog(&fake(b"invalid".to_vec())).is_err());
        assert!(fetch_catalog(&fake(vec![b'X'; MAX_CATALOG_BYTES + 1])).is_err());
    }
}
