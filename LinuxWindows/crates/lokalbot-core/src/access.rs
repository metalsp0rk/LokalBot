//! Consent boundaries shared by the app, engine and CLI.
//!
//! External agents get read access only through marker files under
//! `<root>/control/`, exactly as on macOS (`AgentAccessGate.swift`,
//! `ScreenMemoryAccessGate.swift`). Remote model servers need an approved
//! origin (`RemoteInferenceGate.swift`).

use std::net::IpAddr;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::paths::LibraryPaths;

pub const MEETINGS_MARKER: &str = "agent-access-enabled";
pub const SCREEN_MARKER: &str = "screen-memory-access-enabled";

pub const MEETINGS_DISABLED_MESSAGE: &str = "External agent access is off. Enable it in LokalBot → Settings → Privacy & Data → \"Allow external agents to read your meeting library\".";
pub const SCREEN_DISABLED_MESSAGE: &str = "External screen-memory access is off. Enable it separately in LokalBot → Settings → Privacy & Data → \"Allow external agents to read screen memory\".";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScreenScope {
    Today,
    #[default]
    RecentWeek,
    RetainedHistory,
}

impl ScreenScope {
    pub fn label(self) -> &'static str {
        match self {
            ScreenScope::Today => "Today only",
            ScreenScope::RecentWeek => "Last 7 days",
            ScreenScope::RetainedHistory => "All retained history",
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ScreenProfile {
    scope: ScreenScope,
}

pub fn meetings_access_enabled(paths: &LibraryPaths) -> bool {
    paths.control_dir().join(MEETINGS_MARKER).is_file()
}

pub fn set_meetings_access(paths: &LibraryPaths, enabled: bool) -> std::io::Result<()> {
    let marker = paths.control_dir().join(MEETINGS_MARKER);
    if enabled {
        ensure_private_dir(&paths.control_dir())?;
        crate::library::write_atomic(&marker, uuid::Uuid::new_v4().to_string().as_bytes())?;
        restrict_file(&marker);
        Ok(())
    } else {
        remove_if_exists(&marker)
    }
}

/// `None` when screen access is off. An empty (legacy) marker means all
/// retained history; an unreadable one means the last 7 days.
pub fn screen_access_scope(paths: &LibraryPaths) -> Option<ScreenScope> {
    let marker = paths.control_dir().join(SCREEN_MARKER);
    let bytes = std::fs::read(marker).ok()?;
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Some(ScreenScope::RetainedHistory);
    }
    Some(
        serde_json::from_slice::<ScreenProfile>(&bytes)
            .map(|p| p.scope)
            .unwrap_or(ScreenScope::RecentWeek),
    )
}

pub fn set_screen_access(paths: &LibraryPaths, scope: Option<ScreenScope>) -> std::io::Result<()> {
    let marker = paths.control_dir().join(SCREEN_MARKER);
    match scope {
        Some(scope) => {
            ensure_private_dir(&paths.control_dir())?;
            let json =
                serde_json::to_vec(&ScreenProfile { scope }).map_err(std::io::Error::other)?;
            crate::library::write_atomic(&marker, &json)?;
            restrict_file(&marker);
            Ok(())
        }
        None => remove_if_exists(&marker),
    }
}

fn remove_if_exists(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err),
        _ => Ok(()),
    }
}

fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

fn restrict_file(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// `scheme://host[:port]` of a URL, lowercased; `None` if it does not parse.
pub fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.trim().split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    if authority.is_empty() {
        return None;
    }
    Some(format!(
        "{}://{}",
        scheme.to_lowercase(),
        authority.to_lowercase()
    ))
}

fn host_of(url: &str) -> Option<String> {
    let origin = origin_of(url)?;
    let authority = origin.split_once("://")?.1.to_string();
    if let Some(stripped) = authority.strip_prefix('[') {
        return stripped.split(']').next().map(str::to_string);
    }
    Some(
        authority
            .rsplit_once(':')
            .map(|(h, p)| {
                if p.chars().all(|c| c.is_ascii_digit()) {
                    h.to_string()
                } else {
                    authority.clone()
                }
            })
            .unwrap_or(authority),
    )
}

/// `localhost`, `*.localhost`, `::1` and `127.0.0.0/8`. `0.0.0.0` is not loopback.
pub fn is_loopback_url(url: &str) -> bool {
    let Some(host) = host_of(url) else {
        return false;
    };
    if host == "localhost" || host.ends_with(".localhost") {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => v4.is_loopback(),
        Ok(IpAddr::V6(v6)) => v6.is_loopback(),
        Err(_) => false,
    }
}

/// RFC 1918, link-local, CGNAT (Tailscale), unique-local IPv6, `.local`,
/// `.home.arpa`, `.ts.net` and single-label names.
pub fn is_private_network_url(url: &str) -> bool {
    let Some(host) = host_of(url) else {
        return false;
    };
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => {
            let [a, b, ..] = v4.octets();
            v4.is_private() || v4.is_link_local() || (a == 100 && (64..=127).contains(&b))
        }
        Ok(IpAddr::V6(v6)) => {
            let first = v6.segments()[0];
            (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
        }
        Err(_) => {
            host.ends_with(".local")
                || host.ends_with(".home.arpa")
                || host.ends_with(".ts.net")
                || !host.contains('.')
        }
    }
}

/// Why a server cannot receive context yet, or `None` if it may.
pub fn remote_block_reason(url: &str, approved: &[String]) -> Option<String> {
    if is_loopback_url(url) {
        return None;
    }
    let Some(origin) = origin_of(url) else {
        return Some("The server URL is not valid.".into());
    };
    let https = origin.starts_with("https://");
    if !https && !is_private_network_url(url) {
        return Some(format!(
            "{origin} must use HTTPS before LokalBot can send it context."
        ));
    }
    if approved.iter().any(|a| a.eq_ignore_ascii_case(&origin)) {
        None
    } else {
        Some(format!(
            "Allow sending context to {origin} in Settings → Models first."
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_and_private_detection() {
        assert!(is_loopback_url("http://127.0.0.1:17872/v1"));
        assert!(is_loopback_url("http://localhost:11434"));
        assert!(!is_loopback_url("http://0.0.0.0:8080"));
        assert!(is_private_network_url("http://192.168.1.20:8080"));
        assert!(is_private_network_url("http://100.101.1.2"));
        assert!(is_private_network_url("http://gpu-box.ts.net"));
        assert!(!is_private_network_url("https://api.openai.com"));
    }

    #[test]
    fn remote_servers_need_https_and_approval() {
        assert_eq!(remote_block_reason("http://localhost:1234/v1", &[]), None);
        assert!(
            remote_block_reason("http://example.com/v1", &[])
                .unwrap()
                .contains("HTTPS")
        );
        let approved = vec!["https://openrouter.ai".to_string()];
        assert_eq!(
            remote_block_reason("https://openrouter.ai/api/v1", &approved),
            None
        );
        assert!(remote_block_reason("https://api.openai.com/v1", &approved).is_some());
    }

    #[test]
    fn markers_toggle() {
        let dir = tempfile::tempdir().unwrap();
        let paths = LibraryPaths::new(dir.path());
        assert!(!meetings_access_enabled(&paths));
        set_meetings_access(&paths, true).unwrap();
        assert!(meetings_access_enabled(&paths));
        set_meetings_access(&paths, false).unwrap();
        assert!(!meetings_access_enabled(&paths));
        set_screen_access(&paths, Some(ScreenScope::Today)).unwrap();
        assert_eq!(screen_access_scope(&paths), Some(ScreenScope::Today));
        set_screen_access(&paths, None).unwrap();
        assert_eq!(screen_access_scope(&paths), None);
    }
}
