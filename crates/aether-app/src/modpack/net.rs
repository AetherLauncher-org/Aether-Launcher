//! Restricted payload transport. Every redirect is checked before it is contacted.
use std::io::Read;
use std::time::Duration;
use aether_download::CancellationToken;
use super::FileSource;

pub(super) struct PackNet(CancellationToken);

impl PackNet {
    pub(super) fn new(cancel: CancellationToken) -> Self {
        Self(cancel)
    }
}

impl FileSource for PackNet {
    fn open(&self, url: &str) -> Result<Box<dyn Read>, String> {
        let agent = ureq::Agent::new_with_config(ureq::config::Config::builder()
            .max_redirects(0).timeout_global(Some(Duration::from_secs(600)))
            .user_agent(concat!(
                "AetherLauncher/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/AetherLauncher-org/Aether-Launcher)"
            ))
            .build());
        let mut url = url.to_owned();
        for _ in 0..=5 {
            if self.0.is_cancelled() { return Err(crate::install::CANCELLED.to_owned()); }
            if !aether_content::modpack::allowed_download(&url) { return Err("整合包下载或重定向主机不在白名单中".to_owned()); }
            let response = agent.get(&url).call().map_err(|_| "整合包下载请求失败".to_owned())?;
            if response.status().is_redirection() {
                let location = response.headers().get("location").and_then(|h| h.to_str().ok()).ok_or("整合包重定向无效")?;
                // Accept absolute HTTPS or root-relative redirects, never scheme-relative URLs.
                url = if location.starts_with('/') && !location.starts_with("//") {
                    let authority = url.strip_prefix("https://").and_then(|x| x.split('/').next()).ok_or("重定向地址无效")?;
                    format!("https://{authority}{location}")
                } else { location.to_owned() };
                continue;
            }
            if response.status().as_u16() != 200 { return Err("整合包服务器未返回完整文件".to_owned()); }
            return Ok(Box::new(CancelReader { reader: response.into_body().into_reader(), cancel: self.0.clone() }));
        }
        Err("整合包重定向次数超限".to_owned())
    }
}
struct CancelReader<R> { reader: R, cancel: CancellationToken }
impl<R: Read> Read for CancelReader<R> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.is_cancelled() { return Err(std::io::Error::other("cancelled")); }
        self.reader.read(bytes)
    }
}
