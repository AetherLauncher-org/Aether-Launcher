//! Small things that differ per operating system.

use std::path::{Path, PathBuf};

/// Windows' extended-length prefix (`\\?\C:\…`) is something Explorer's opener dislikes.
/// Network paths keep theirs.
#[must_use]
pub fn plain(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

/// Hands a folder to the platform's file manager. The path is one argument, never
/// interpreted by a shell.
///
/// # Errors
///
/// When the file manager cannot be started.
pub fn open_in_file_manager(folder: &Path) -> Result<(), String> {
    let mut command = if cfg!(target_os = "windows") {
        let mut command = std::process::Command::new("explorer.exe");
        command.arg(folder);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("open");
        command.arg(folder);
        command
    } else {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(folder);
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|_| "cannot open the file manager".to_owned())
}

/// Hands `uri` to the platform's default-browser opener. The URL is one argument, never
/// interpreted by a shell.
///
/// # Errors
///
/// When the browser cannot be started.
pub fn open_in_browser(uri: &str) -> Result<(), String> {
    let mut command = if cfg!(target_os = "windows") {
        let mut command = std::process::Command::new("rundll32");
        command.args(["url.dll,FileProtocolHandler", uri]);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("open");
        command.arg(uri);
        command
    } else {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(uri);
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|_| "cannot open the default browser".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extended_length_prefix_is_dropped_but_network_paths_are_kept() {
        assert_eq!(
            plain(Path::new(r"\\?\C:\Users\me\x")),
            Path::new(r"C:\Users\me\x")
        );
        assert_eq!(
            plain(Path::new(r"\\?\UNC\server\share")),
            Path::new(r"\\?\UNC\server\share")
        );
        assert_eq!(plain(Path::new(r"C:\plain")), Path::new(r"C:\plain"));
    }
}
