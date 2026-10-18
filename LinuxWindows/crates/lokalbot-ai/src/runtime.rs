//! The built-in llama.cpp runtime (`Engines/LlamaServer.swift`,
//! `CLISupport/LlamaServerRole.swift`, `InferenceBroker.swift`).
//!
//! macOS bundles a source build; here the pinned prebuilt release of
//! ggml-org/llama.cpp is downloaded once, verified, and unpacked into
//! `<root>/runtimes/llama-cpp/<build>-<asset tag>/`. Each role (Think,
//! embeddings, autocomplete) is its own `llama-server` subprocess on a fixed
//! loopback port with a random bearer token. A [`Lease`] keeps a server loaded;
//! after the last lease drops it lingers for the role's idle time, then stops.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, anyhow};
use parking_lot::{Condvar, Mutex};
use serde::{Deserialize, Serialize};

use lokalbot_core::{LibraryPaths, Settings};

use crate::catalog::{self, ModelEntry};
use crate::download::{self, DownloadSpec};
use crate::error::AiError;
use crate::process;

/// The llama.cpp release every platform runs (macOS builds the same tag).
pub const LLAMA_BUILD: &str = "b11474";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlamaBackend {
    Cpu,
    Vulkan,
}

impl LlamaBackend {
    pub fn label(self) -> &'static str {
        match self {
            LlamaBackend::Cpu => "CPU",
            LlamaBackend::Vulkan => "Vulkan",
        }
    }
}

/// One prebuilt release archive (sizes and digests from the GitHub release API).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeAsset {
    pub backend: LlamaBackend,
    /// The release's platform tag, e.g. `win-vulkan-x64`.
    pub tag: &'static str,
    pub name: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

impl RuntimeAsset {
    pub fn url(&self) -> String {
        format!(
            "https://github.com/ggml-org/llama.cpp/releases/download/{LLAMA_BUILD}/{}",
            self.name
        )
    }

    fn folder(&self) -> String {
        format!("{LLAMA_BUILD}-{}", self.tag)
    }
}

#[cfg(windows)]
const ASSETS: &[RuntimeAsset] = &[
    RuntimeAsset {
        backend: LlamaBackend::Vulkan,
        tag: "win-vulkan-x64",
        name: "llama-b11474-bin-win-vulkan-x64.zip",
        size: 33_380_391,
        sha256: "b3632a0615f200d89ba45526065fa298f462dc40cc8a214e058c7b30153fc869",
    },
    RuntimeAsset {
        backend: LlamaBackend::Cpu,
        tag: "win-cpu-x64",
        name: "llama-b11474-bin-win-cpu-x64.zip",
        size: 19_440_991,
        sha256: "5a39102a1b27d75ab8a9d777c6cf2ce373c468265dc1797a2566dec085501a95",
    },
];

#[cfg(not(windows))]
const ASSETS: &[RuntimeAsset] = &[
    RuntimeAsset {
        backend: LlamaBackend::Vulkan,
        tag: "ubuntu-vulkan-x64",
        name: "llama-b11474-bin-ubuntu-vulkan-x64.tar.gz",
        size: 31_684_079,
        sha256: "ee414b7a48d7765e7d072e023cdc85dbf5c4ee582ea0058aec141030e7dad56e",
    },
    RuntimeAsset {
        backend: LlamaBackend::Cpu,
        tag: "ubuntu-x64",
        name: "llama-b11474-bin-ubuntu-x64.tar.gz",
        size: 17_737_147,
        sha256: "98abe0fac169db55712ce96b0e30adf99fa47fa5386fddef8fef84fe21f4a690",
    },
];

/// The assets for this platform.
pub fn assets() -> &'static [RuntimeAsset] {
    ASSETS
}

pub fn asset(backend: LlamaBackend) -> Option<&'static RuntimeAsset> {
    ASSETS.iter().find(|a| a.backend == backend)
}

const SERVER_BINARY: &str = if cfg!(windows) {
    "llama-server.exe"
} else {
    "llama-server"
};

// ------------------------------------------------------------------- roles

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// Think: summaries, digests, Ask.
    Main,
    Embedder,
    Autocomplete,
}

impl Role {
    pub const ALL: [Role; 3] = [Role::Main, Role::Embedder, Role::Autocomplete];

    /// The ports the CLI and MCP clients rely on; they never move.
    pub fn release_port(self) -> u16 {
        match self {
            Role::Main => 17_872,
            Role::Embedder => 17_873,
            Role::Autocomplete => 17_874,
        }
    }

    /// `release_port` plus `LOKALBOT_PORT_OFFSET`, so a development copy or a
    /// test run never fights the installed app for a port.
    pub fn port(self) -> u16 {
        self.release_port().saturating_add(port_offset())
    }

    pub fn context_tokens(self) -> u32 {
        match self {
            Role::Main => 32_768,
            Role::Embedder => 2_048,
            Role::Autocomplete => 4_096,
        }
    }

    pub fn idle_timeout(self) -> Duration {
        match self {
            Role::Main | Role::Embedder => Duration::from_secs(600),
            Role::Autocomplete => Duration::from_secs(900),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Role::Main => "main",
            Role::Embedder => "embedder",
            Role::Autocomplete => "autocomplete",
        }
    }

    /// Role arguments, keeping only flags this build understands.
    fn extra_args(self, flags: &[String]) -> Vec<String> {
        let has = |flag: &str| flags.iter().any(|f| f == flag);
        let mut args: Vec<String> = Vec::new();
        match self {
            Role::Main => {
                if has("--cache-ram") {
                    args.extend(["--cache-ram".into(), "2048".into()]);
                }
                if has("--reasoning") {
                    args.extend(["--reasoning".into(), "on".into()]);
                }
            }
            // EmbeddingGemma carries its own pooling; as a bidirectional
            // encoder each input must fit one micro-batch.
            Role::Embedder => {
                args.extend(
                    [
                        "--embeddings",
                        "-b",
                        "2048",
                        "-ub",
                        "2048",
                        "--parallel",
                        "1",
                    ]
                    .map(String::from),
                );
            }
            Role::Autocomplete => {
                args.extend(["--parallel".into(), "1".into()]);
                if has("--cache-ram") {
                    args.extend(["--cache-ram".into(), "512".into()]);
                }
            }
        }
        args
    }
}

pub fn port_offset() -> u16 {
    std::env::var("LOKALBOT_PORT_OFFSET")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0)
}

// ------------------------------------------------------------- installation

/// `install.json` inside an installed runtime folder.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct InstallRecord {
    build: String,
    asset: String,
    sha256: String,
    backend: LlamaBackend,
    /// `llama-server` relative to the runtime folder.
    server: String,
    #[serde(default)]
    version: String,
    /// The flags from `--help` that LokalBot passes conditionally.
    #[serde(default)]
    flags: Vec<String>,
}

/// `backend.json`: the result of the last auto-detection.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct BackendChoice {
    build: String,
    backend: LlamaBackend,
    #[serde(default)]
    device: Option<String>,
}

const CONDITIONAL_FLAGS: [&str; 4] = ["--reasoning", "--cache-ram", "--no-webui", "--jinja"];

struct Installer {
    paths: LibraryPaths,
    preference: Mutex<String>,
    /// The backend this session runs, once known.
    active: Mutex<Option<LlamaBackend>>,
    lock: Mutex<()>,
}

impl Installer {
    fn root(&self) -> PathBuf {
        self.paths.runtimes_dir().join("llama-cpp")
    }

    fn folder(&self, asset: &RuntimeAsset) -> PathBuf {
        self.root().join(asset.folder())
    }

    fn record(&self, backend: LlamaBackend) -> Option<(InstallRecord, PathBuf)> {
        let asset = asset(backend)?;
        let folder = self.folder(asset);
        let record: InstallRecord =
            serde_json::from_slice(&std::fs::read(folder.join("install.json")).ok()?).ok()?;
        let binary = folder.join(&record.server);
        (record.sha256 == asset.sha256 && record.build == LLAMA_BUILD && binary.is_file())
            .then_some((record, binary))
    }

    fn saved_choice(&self) -> Option<BackendChoice> {
        let choice: BackendChoice =
            serde_json::from_slice(&std::fs::read(self.root().join("backend.json")).ok()?).ok()?;
        (choice.build == LLAMA_BUILD).then_some(choice)
    }

    fn save_choice(&self, backend: LlamaBackend, device: Option<String>) {
        let choice = BackendChoice {
            build: LLAMA_BUILD.into(),
            backend,
            device,
        };
        if let Ok(bytes) = serde_json::to_vec_pretty(&choice) {
            let _ = lokalbot_core::library::write_atomic(&self.root().join("backend.json"), &bytes);
        }
    }

    fn preference(&self) -> String {
        self.preference.lock().trim().to_ascii_lowercase()
    }

    /// Downloads and unpacks one backend unless it is already installed.
    fn install(
        &self,
        backend: LlamaBackend,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<(InstallRecord, PathBuf)> {
        if let Some(installed) = self.record(backend) {
            return Ok(installed);
        }
        let asset = asset(backend).ok_or_else(|| {
            anyhow!(
                "llama.cpp has no {} build for this platform",
                backend.label()
            )
        })?;
        let downloads = self.root().join(".downloads");
        let archive = downloads.join(asset.name);
        let spec = DownloadSpec {
            url: asset.url(),
            sha256: asset.sha256.into(),
            size: asset.size,
            dest: archive.clone(),
        };
        download::download(&spec, cancel, progress)
            .with_context(|| format!("downloading the llama.cpp {} runtime", backend.label()))?;

        let folder = self.folder(asset);
        let staging = self
            .root()
            .join(format!(".staging-{}", uuid::Uuid::new_v4().simple()));
        let result = (|| -> Result<(InstallRecord, PathBuf)> {
            download::extract_archive(&archive, &staging)?;
            let server = find_file(&staging, SERVER_BINARY, 4)
                .ok_or_else(|| anyhow!("the llama.cpp archive has no {SERVER_BINARY}"))?;
            let relative = server
                .strip_prefix(&staging)?
                .to_string_lossy()
                .replace('\\', "/");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                let mut perms = std::fs::metadata(&server)?.permissions();
                perms.set_mode(perms.mode() | 0o755);
                std::fs::set_permissions(&server, perms)?;
            }
            let (version, flags) = probe_capabilities(&server);
            let record = InstallRecord {
                build: LLAMA_BUILD.into(),
                asset: asset.name.into(),
                sha256: asset.sha256.into(),
                backend,
                server: relative.clone(),
                version,
                flags,
            };
            std::fs::write(
                staging.join("install.json"),
                serde_json::to_vec_pretty(&record)?,
            )?;
            if folder.exists() {
                std::fs::remove_dir_all(&folder)
                    .with_context(|| format!("replacing {}", folder.display()))?;
            }
            std::fs::rename(&staging, &folder)
                .with_context(|| format!("installing {}", folder.display()))?;
            Ok((record, folder.join(relative)))
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&staging);
        }
        let _ = std::fs::remove_file(&archive);
        let _ = std::fs::remove_file(download::sidecar_path(&archive));
        result
    }

    /// The backend to run. `auto` takes Vulkan only when the Vulkan build
    /// lists a real GPU (a loader alone, or a software rasterizer, is not one).
    /// With `allow_download` false, only installed builds are considered.
    fn resolve(
        &self,
        allow_download: bool,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<(LlamaBackend, InstallRecord, PathBuf)> {
        let _guard = self.lock.lock();
        let get = |backend: LlamaBackend,
                   progress: &mut dyn FnMut(u64, u64)|
         -> Result<(InstallRecord, PathBuf)> {
            if allow_download {
                self.install(backend, cancel, progress)
            } else {
                self.record(backend).ok_or_else(|| {
                    AiError::Unavailable("The local model runtime isn't installed yet. Download it under Settings → Models.".into()).into()
                })
            }
        };
        let preference = self.preference();
        let active = *self.active.lock();
        if let Some(active) = active
            && (preference == "auto" || preference == active.label().to_ascii_lowercase())
        {
            let (record, binary) = get(active, progress)?;
            return Ok((active, record, binary));
        }
        let backend = match preference.as_str() {
            "cpu" => LlamaBackend::Cpu,
            "vulkan" => LlamaBackend::Vulkan,
            _ => match self.saved_choice() {
                Some(choice) if self.record(choice.backend).is_some() => choice.backend,
                _ if !vulkan_loader_present() => LlamaBackend::Cpu,
                _ => {
                    let vulkan = if allow_download {
                        self.install(LlamaBackend::Vulkan, cancel, progress).ok()
                    } else {
                        self.record(LlamaBackend::Vulkan)
                    };
                    match vulkan.as_ref().and_then(|(_, binary)| gpu_device(binary)) {
                        Some(device) => {
                            log::info!("llama.cpp: using Vulkan on {device}");
                            self.save_choice(LlamaBackend::Vulkan, Some(device));
                            LlamaBackend::Vulkan
                        }
                        None => {
                            if vulkan.is_some() {
                                log::info!("llama.cpp: no Vulkan GPU found; using the CPU build");
                                self.save_choice(LlamaBackend::Cpu, None);
                                // Only fetched to look for a GPU; the choice is remembered.
                                if let Some(asset) = asset(LlamaBackend::Vulkan) {
                                    let _ = std::fs::remove_dir_all(self.folder(asset));
                                }
                            }
                            LlamaBackend::Cpu
                        }
                    }
                }
            },
        };
        let (record, binary) = get(backend, progress)?;
        *self.active.lock() = Some(backend);
        Ok((backend, record, binary))
    }

    /// After a Vulkan server failed to start under `auto`.
    fn fall_back_to_cpu(&self) {
        *self.active.lock() = Some(LlamaBackend::Cpu);
        self.save_choice(LlamaBackend::Cpu, None);
    }
}

fn find_file(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut folders = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            folders.push(path);
        } else if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
    }
    if depth == 0 {
        return None;
    }
    folders.sort();
    folders
        .into_iter()
        .find_map(|folder| find_file(&folder, name, depth - 1))
}

fn server_command(binary: &Path) -> Command {
    let mut command = Command::new(binary);
    if let Some(dir) = binary.parent() {
        command.current_dir(dir);
        #[cfg(target_os = "linux")]
        {
            let mut value = dir.as_os_str().to_os_string();
            if let Some(existing) = std::env::var_os("LD_LIBRARY_PATH").filter(|v| !v.is_empty()) {
                value.push(":");
                value.push(existing);
            }
            command.env("LD_LIBRARY_PATH", value);
        }
    }
    command
}

/// `--version` and the conditional flags `--help` lists.
fn probe_capabilities(binary: &Path) -> (String, Vec<String>) {
    let version = process::run_capture(
        {
            let mut c = server_command(binary);
            c.arg("--version");
            c
        },
        Duration::from_secs(20),
    )
    .map(|out| {
        format!("{}\n{}", out.stdout, out.stderr)
            .lines()
            .find(|l| l.contains("version"))
            .unwrap_or_default()
            .trim()
            .to_string()
    })
    .unwrap_or_default();
    let help = process::run_capture(
        {
            let mut c = server_command(binary);
            c.arg("--help");
            c
        },
        Duration::from_secs(20),
    )
    .map(|out| format!("{}\n{}", out.stdout, out.stderr))
    .unwrap_or_default();
    (version, supported_flags(&help))
}

fn supported_flags(help: &str) -> Vec<String> {
    CONDITIONAL_FLAGS
        .iter()
        .filter(|flag| {
            help.match_indices(*flag).any(|(at, _)| {
                let before = help[..at].chars().next_back();
                let after = help[at + flag.len()..].chars().next();
                before.is_none_or(|c| c.is_whitespace() || c == ',')
                    && after.is_none_or(|c| c.is_whitespace() || c == ',' || c == '=')
            })
        })
        .map(|f| f.to_string())
        .collect()
}

fn vulkan_loader_present() -> bool {
    #[cfg(windows)]
    {
        let root = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        root.join("System32").join("vulkan-1.dll").is_file()
    }
    #[cfg(not(windows))]
    {
        [
            "/usr/lib/x86_64-linux-gnu/libvulkan.so.1",
            "/lib/x86_64-linux-gnu/libvulkan.so.1",
            "/usr/lib64/libvulkan.so.1",
            "/usr/lib/libvulkan.so.1",
            "/usr/local/lib/libvulkan.so.1",
        ]
        .iter()
        .any(|p| Path::new(p).exists())
    }
}

/// The first GPU the Vulkan build reports, ignoring software rasterizers.
fn gpu_device(binary: &Path) -> Option<String> {
    let mut command = server_command(binary);
    command.arg("--list-devices");
    let out = process::run_capture(command, Duration::from_secs(30)).ok()?;
    if out.status.is_none() {
        log::warn!("llama.cpp: listing Vulkan devices timed out; using the CPU build");
        return None;
    }
    parse_gpu_devices(&format!("{}\n{}", out.stdout, out.stderr))
        .into_iter()
        .next()
}

fn parse_gpu_devices(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter_map(|line| {
            let (name, rest) = line.split_once(':')?;
            let is_device = name.starts_with("Vulkan")
                && name[6..].chars().all(|c| c.is_ascii_digit())
                && name.len() > 6;
            let lower = rest.to_ascii_lowercase();
            let software = ["llvmpipe", "lavapipe", "swiftshader", "software rasterizer"]
                .iter()
                .any(|s| lower.contains(s));
            (is_device && !software).then(|| rest.trim().to_string())
        })
        .collect()
}

// ------------------------------------------------------------------ servers

/// `<root>/llama-server-<port>.json`: lets a later run recognize (and stop)
/// a helper an earlier run left behind, and lets the CLI reach the app's
/// server. Written 0600 on Unix; it holds the bearer token.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServerMarker {
    pub pid: u32,
    pub port: u16,
    pub binary_path: String,
    pub model_path: String,
    pub context_tokens: u32,
    #[serde(default)]
    pub extra_args: Vec<String>,
    pub authentication_token: String,
    #[serde(rename = "ownerPID")]
    pub owner_pid: Option<u32>,
}

pub fn marker_path(paths: &LibraryPaths, port: u16) -> PathBuf {
    paths.root().join(format!("llama-server-{port}.json"))
}

/// The marker of the server on `port`, for callers that only read (CLI).
pub fn read_marker(paths: &LibraryPaths, port: u16) -> Option<ServerMarker> {
    serde_json::from_slice(&std::fs::read(marker_path(paths, port)).ok()?).ok()
}

fn write_marker(paths: &LibraryPaths, marker: &ServerMarker) {
    let path = marker_path(paths, marker.port);
    let Ok(bytes) = serde_json::to_vec_pretty(marker) else {
        return;
    };
    if lokalbot_core::library::write_atomic(&path, &bytes).is_err() {
        log::warn!(
            "llama-server: could not record the helper marker for port {}",
            marker.port
        );
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
}

fn remove_marker_if(paths: &LibraryPaths, port: u16, pid: u32) {
    if read_marker(paths, port).is_some_and(|m| m.pid == pid) {
        let _ = std::fs::remove_file(marker_path(paths, port));
    }
}

/// 64 lowercase hex characters from two random UUIDs, as on macOS.
fn new_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// The tail of llama-server's stderr (`LlamaServerOutputTail`). Before the
/// first healthy check every line is kept so a failed start can say why;
/// afterwards only crash lines, since request errors can echo private text.
#[derive(Default)]
struct OutputTail {
    lines: Vec<String>,
    ready: bool,
    finished: bool,
}

const TAIL_LINES: usize = 12;
const TAIL_LINE_CHARS: usize = 300;
const CRASH_MARKERS: [&str; 8] = [
    "ggml_assert",
    "ggml_vulkan",
    "out of memory",
    "insufficient memory",
    "failed to allocate",
    "terminating",
    "abort",
    "segmentation fault",
];

impl OutputTail {
    fn keep(&mut self, raw: &str) {
        let mut line = raw.trim();
        // "0.00.077.284 E gguf_init_from_reader: …": drop the time and level.
        if let Some((stamp, rest)) = line.split_once(' ')
            && !stamp.is_empty()
            && stamp.chars().all(|c| c.is_ascii_digit() || c == '.')
            && rest.len() > 2
            && rest.as_bytes()[0].is_ascii_uppercase()
            && rest.as_bytes()[1] == b' '
        {
            line = rest[2..].trim();
        }
        if line.is_empty() {
            return;
        }
        if self.ready {
            let lower = line.to_lowercase();
            if !CRASH_MARKERS.iter().any(|m| lower.contains(m)) || lower.contains("last read") {
                return;
            }
        }
        self.lines
            .push(line.chars().take(TAIL_LINE_CHARS).collect());
        if self.lines.len() > TAIL_LINES {
            let excess = self.lines.len() - TAIL_LINES;
            self.lines.drain(..excess);
        }
    }

    /// The first line reporting an error, else the last line.
    fn reason(&self) -> Option<String> {
        self.lines
            .iter()
            .find(|l| {
                let lower = l.to_lowercase();
                ["error", "failed", "ggml_assert"]
                    .iter()
                    .any(|k| lower.contains(k))
            })
            .or_else(|| self.lines.last())
            .cloned()
    }
}

struct Running {
    pid: u32,
    /// `None` for a helper adopted from an earlier run or borrowed from a live one.
    child: Option<Child>,
    /// A live LokalBot elsewhere owns it: never stopped from here.
    borrowed: bool,
    model_path: PathBuf,
    model_id: String,
    binary: PathBuf,
    token: String,
    tail: Arc<Mutex<OutputTail>>,
    backend: LlamaBackend,
}

enum Phase {
    Stopped,
    Starting,
    Running(Box<Running>),
}

struct ServerState {
    phase: Phase,
    leases: usize,
    idle_since: Option<Instant>,
    /// The model `lease()` starts when nothing is loaded.
    desired: Option<(String, PathBuf)>,
}

struct ServerInner {
    role: Role,
    port: u16,
    installer: Arc<Installer>,
    state: Mutex<ServerState>,
    changed: Condvar,
}

/// What a resource view shows for one role.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerStatus {
    pub role: Role,
    pub port: u16,
    pub running: bool,
    pub model_id: Option<String>,
    pub leases: usize,
    pub pid: Option<u32>,
    pub backend: Option<LlamaBackend>,
}

/// One role's `llama-server`.
#[derive(Clone)]
pub struct LlamaServer {
    inner: Arc<ServerInner>,
}

/// Keeps a server loaded; dropping the last lease starts the idle timer.
pub struct Lease {
    server: Arc<ServerInner>,
    port: u16,
    token: String,
    model_id: String,
    model_path: PathBuf,
}

impl Lease {
    /// `http://127.0.0.1:<port>/v1`
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port)
    }

    /// `http://127.0.0.1:<port>`, for `/health`, `/tokenize` and `/props`.
    pub fn root_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    /// A request could not reach the server: if it has died, forget it so
    /// the next lease starts a fresh one.
    pub fn mark_unreachable(&self) {
        let mut state = self.server.state.lock();
        let dead = match &mut state.phase {
            Phase::Running(running) => {
                let exited = match running.child.as_mut() {
                    Some(child) => !matches!(child.try_wait(), Ok(None)),
                    None => !process::pid_alive(running.pid),
                };
                exited || !health(self.port, &running.token)
            }
            _ => false,
        };
        if !dead {
            return;
        }
        log::warn!(
            "llama-server {} stopped answering; it will be restarted",
            self.server.role.label()
        );
        let phase = std::mem::replace(&mut state.phase, Phase::Stopped);
        drop(state);
        if let Phase::Running(running) = phase {
            self.server.stop_running(*running);
        }
        self.server.changed.notify_all();
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let mut state = self.server.state.lock();
        state.leases = state.leases.saturating_sub(1);
        if state.leases == 0 {
            state.idle_since = Some(Instant::now());
        }
        drop(state);
        self.server.changed.notify_all();
    }
}

fn health(port: u16, token: &str) -> bool {
    let agent = crate::http::agent("http://127.0.0.1", Duration::from_secs(2));
    crate::http::get(
        &agent,
        &format!("http://127.0.0.1:{port}/health"),
        Some(token),
    )
    .is_ok_and(|r| r.status == 200)
}

impl LlamaServer {
    pub fn role(&self) -> Role {
        self.inner.role
    }

    pub fn port(&self) -> u16 {
        self.inner.port
    }

    /// `http://127.0.0.1:<port>/v1`
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.inner.port)
    }

    /// Sets the model [`lease`](Self::lease) loads when none is running.
    pub fn set_model(&self, entry: &ModelEntry) {
        let path = catalog::model_path(&self.inner.installer.paths, entry);
        self.inner.state.lock().desired = Some((entry.id.to_string(), path));
    }

    /// Leases the running model, or starts the one from [`set_model`](Self::set_model).
    pub fn lease(&self) -> Result<Lease> {
        let desired = {
            let state = self.inner.state.lock();
            match &state.phase {
                Phase::Running(running) => {
                    Some((running.model_id.clone(), running.model_path.clone()))
                }
                _ => state.desired.clone(),
            }
        };
        let (id, path) = desired.ok_or(AiError::NoModel)?;
        self.inner.lease_path(&id, &path)
    }

    /// Leases `entry`, starting (or switching to) it as needed. A switch waits
    /// until every lease on the current model is released.
    pub fn lease_model(&self, entry: &ModelEntry) -> Result<Lease> {
        let paths = &self.inner.installer.paths;
        // Hashes once when the file has no digest sidecar yet.
        if !catalog::verify_downloaded(paths, entry)? {
            return Err(AiError::ModelMissing(entry.display_name.into()).into());
        }
        let path = catalog::model_path(paths, entry);
        self.inner.state.lock().desired = Some((entry.id.to_string(), path.clone()));
        self.inner.lease_path(entry.id, &path)
    }

    /// Stops the server now (leases still held will see it restart).
    pub fn stop(&self) {
        let phase = std::mem::replace(&mut self.inner.state.lock().phase, Phase::Stopped);
        if let Phase::Running(running) = phase {
            self.inner.stop_running(*running);
        }
        self.inner.changed.notify_all();
    }

    pub fn status(&self) -> ServerStatus {
        let state = self.inner.state.lock();
        let running = match &state.phase {
            Phase::Running(r) => Some(r),
            _ => None,
        };
        ServerStatus {
            role: self.inner.role,
            port: self.inner.port,
            running: running.is_some(),
            model_id: running.map(|r| r.model_id.clone()),
            leases: state.leases,
            pid: running.map(|r| r.pid),
            backend: running.map(|r| r.backend),
        }
    }
}

impl ServerInner {
    fn lease_path(self: &Arc<Self>, model_id: &str, model_path: &Path) -> Result<Lease> {
        enum Next {
            Lease,
            Forget,
            Wait,
            Start,
        }
        let mut state = self.state.lock();
        loop {
            let busy = state.leases > 0;
            let next = match &mut state.phase {
                Phase::Running(running) if running.model_path == model_path => {
                    let alive = match running.child.as_mut() {
                        Some(child) => matches!(child.try_wait(), Ok(None)),
                        None => process::pid_alive(running.pid),
                    };
                    if alive { Next::Lease } else { Next::Forget }
                }
                Phase::Starting => Next::Wait,
                // Another model is in use; switch once its cohort finishes.
                Phase::Running(_) if busy => Next::Wait,
                _ => Next::Start,
            };
            match next {
                Next::Lease => {
                    state.leases += 1;
                    state.idle_since = None;
                    let Phase::Running(running) = &state.phase else {
                        unreachable!()
                    };
                    return Ok(Lease {
                        server: self.clone(),
                        port: self.port,
                        token: running.token.clone(),
                        model_id: running.model_id.clone(),
                        model_path: running.model_path.clone(),
                    });
                }
                Next::Forget => {
                    log::warn!("llama-server {} exited; restarting", self.role.label());
                    if let Phase::Running(running) =
                        std::mem::replace(&mut state.phase, Phase::Stopped)
                    {
                        remove_marker_if(&self.installer.paths, self.port, running.pid);
                    }
                }
                Next::Wait => {
                    self.changed.wait_for(&mut state, Duration::from_secs(1));
                }
                Next::Start => {
                    let previous = std::mem::replace(&mut state.phase, Phase::Starting);
                    drop(state);
                    if let Phase::Running(running) = previous {
                        self.stop_running(*running);
                    }
                    let started = self.start(model_id, model_path);
                    state = self.state.lock();
                    let result = match started {
                        Ok(running) => {
                            state.leases += 1;
                            state.idle_since = None;
                            let lease = Lease {
                                server: self.clone(),
                                port: self.port,
                                token: running.token.clone(),
                                model_id: running.model_id.clone(),
                                model_path: running.model_path.clone(),
                            };
                            state.phase = Phase::Running(Box::new(running));
                            Ok(lease)
                        }
                        Err(err) => {
                            state.phase = Phase::Stopped;
                            Err(err)
                        }
                    };
                    drop(state);
                    self.changed.notify_all();
                    return result;
                }
            }
        }
    }

    fn start(&self, model_id: &str, model_path: &Path) -> Result<Running> {
        let installer = &self.installer;
        let never = AtomicBool::new(false);
        let (backend, record, binary) = installer.resolve(true, &never, &mut |_, _| {})?;
        match self.launch(model_id, model_path, backend, &record, &binary) {
            Ok(running) => Ok(running),
            Err(err) if backend == LlamaBackend::Vulkan && installer.preference() == "auto" => {
                log::warn!(
                    "llama-server {} failed on Vulkan ({err:#}); falling back to the CPU build",
                    self.role.label()
                );
                installer.fall_back_to_cpu();
                let (backend, record, binary) = installer.resolve(true, &never, &mut |_, _| {})?;
                self.launch(model_id, model_path, backend, &record, &binary)
            }
            Err(err) => Err(err),
        }
    }

    fn launch(
        &self,
        model_id: &str,
        model_path: &Path,
        backend: LlamaBackend,
        record: &InstallRecord,
        binary: &Path,
    ) -> Result<Running> {
        let paths = &self.installer.paths;
        let extra_args = self.role.extra_args(&record.flags);
        let context = self.role.context_tokens();
        if let Some(running) =
            self.reclaim(model_id, model_path, binary, context, &extra_args, backend)?
        {
            return Ok(running);
        }
        if std::net::TcpListener::bind(("127.0.0.1", self.port)).is_err() {
            return Err(AiError::ServerStart(format!(
                "port {} is already in use. Close the other LokalBot instance or local server and try again",
                self.port
            ))
            .into());
        }
        let token = new_token();
        let mut args: Vec<String> = vec![
            "-m".into(),
            model_path.to_string_lossy().into_owned(),
            "--host".into(),
            "127.0.0.1".into(),
            "--port".into(),
            self.port.to_string(),
            "-c".into(),
            context.to_string(),
            "-ngl".into(),
            "99".into(),
        ];
        if record.flags.iter().any(|f| f == "--jinja") || record.flags.is_empty() {
            args.push("--jinja".into());
        }
        if record.flags.iter().any(|f| f == "--no-webui") {
            args.push("--no-webui".into());
        }
        args.extend(["--api-key".into(), token.clone()]);
        args.extend(extra_args.iter().cloned());

        let mut command = server_command(binary);
        command
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = process::spawn_tied(command).map_err(|err| {
            AiError::ServerStart(format!("could not run {}: {err}", binary.display()))
        })?;
        let pid = child.id();
        let tail = Arc::new(Mutex::new(OutputTail::default()));
        if let Some(stderr) = child.stderr.take() {
            let tail = tail.clone();
            let name = format!("llama-server-{}-stderr", self.role.label());
            let _ = std::thread::Builder::new().name(name).spawn(move || {
                use std::io::BufRead as _;
                let mut reader = std::io::BufReader::new(stderr);
                let mut line = Vec::new();
                loop {
                    line.clear();
                    match reader.read_until(b'\n', &mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => tail.lock().keep(&String::from_utf8_lossy(&line)),
                    }
                }
                tail.lock().finished = true;
            });
        }
        write_marker(
            paths,
            &ServerMarker {
                pid,
                port: self.port,
                binary_path: binary.to_string_lossy().into_owned(),
                model_path: model_path.to_string_lossy().into_owned(),
                context_tokens: context,
                extra_args: extra_args.clone(),
                authentication_token: token.clone(),
                owner_pid: Some(std::process::id()),
            },
        );
        log::info!(
            "llama-server {} starting pid={pid} port={} model={} backend={}",
            self.role.label(),
            self.port,
            model_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            backend.label()
        );

        let failure = |summary: &str, tail: &Mutex<OutputTail>| -> anyhow::Error {
            // Let the reader catch the last lines of an exited process.
            for _ in 0..10 {
                if tail.lock().finished {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            let tail = tail.lock();
            if !tail.lines.is_empty() {
                log::warn!(
                    "llama-server startup output port={}: {}",
                    self.port,
                    tail.lines.join(" | ")
                );
            }
            let message = match tail.reason() {
                Some(reason) => format!("{summary}: {reason}"),
                None => summary.to_string(),
            };
            AiError::ServerStart(message).into()
        };
        // Loading a large model can take a while.
        for _ in 0..240 {
            std::thread::sleep(Duration::from_millis(500));
            if !matches!(child.try_wait(), Ok(None)) {
                remove_marker_if(paths, self.port, pid);
                return Err(failure("llama-server exited during startup", &tail));
            }
            if health(self.port, &token) {
                tail.lock().ready = true;
                return Ok(Running {
                    pid,
                    child: Some(child),
                    borrowed: false,
                    model_path: model_path.to_path_buf(),
                    model_id: model_id.to_string(),
                    binary: binary.to_path_buf(),
                    token,
                    tail,
                    backend,
                });
            }
        }
        process::stop_child(&mut child, Duration::from_secs(2));
        remove_marker_if(paths, self.port, pid);
        Err(failure("server did not become healthy in time", &tail))
    }

    /// Deals with a helper recorded on this port. A healthy one with the same
    /// configuration is reused: adopted when its LokalBot is gone, borrowed
    /// when that LokalBot still runs. A mismatched orphan is stopped.
    fn reclaim(
        &self,
        model_id: &str,
        model_path: &Path,
        binary: &Path,
        context: u32,
        extra_args: &[String],
        backend: LlamaBackend,
    ) -> Result<Option<Running>> {
        let paths = &self.installer.paths;
        let Some(mut marker) = read_marker(paths, self.port) else {
            return Ok(None);
        };
        let me = std::process::id();
        let helper_alive = process::pid_alive(marker.pid)
            && process::exe_path(marker.pid)
                .is_some_and(|exe| same_file(&exe, Path::new(&marker.binary_path)));
        if !helper_alive {
            let _ = std::fs::remove_file(marker_path(paths, self.port));
            return Ok(None);
        }
        let owner_alive = marker
            .owner_pid
            .is_some_and(|owner| owner != me && process::pid_alive(owner));
        let matches = same_file(Path::new(&marker.binary_path), binary)
            && Path::new(&marker.model_path) == model_path
            && marker.context_tokens == context
            && marker.extra_args == extra_args
            && health(self.port, &marker.authentication_token);
        if matches {
            if !owner_alive {
                marker.owner_pid = Some(me);
                write_marker(paths, &marker);
            }
            log::info!(
                "llama-server {} reusing pid={} ({})",
                self.role.label(),
                marker.pid,
                if owner_alive {
                    "owned by another LokalBot"
                } else {
                    "left by an earlier run"
                }
            );
            let tail = Arc::new(Mutex::new(OutputTail {
                ready: true,
                ..Default::default()
            }));
            return Ok(Some(Running {
                pid: marker.pid,
                child: None,
                borrowed: owner_alive,
                model_path: model_path.to_path_buf(),
                model_id: model_id.to_string(),
                binary: binary.to_path_buf(),
                token: marker.authentication_token,
                tail,
                backend,
            }));
        }
        if owner_alive {
            return Err(AiError::ServerStart(format!(
                "another LokalBot instance is using port {} with a different model",
                self.port
            ))
            .into());
        }
        log::info!(
            "llama-server {}: stopping an orphaned helper pid={}",
            self.role.label(),
            marker.pid
        );
        process::terminate(marker.pid, Duration::from_secs(2));
        let _ = std::fs::remove_file(marker_path(paths, self.port));
        Ok(None)
    }

    fn stop_running(&self, mut running: Running) {
        let paths = &self.installer.paths;
        if running.borrowed {
            return;
        }
        match running.child.as_mut() {
            Some(child) => process::stop_child(child, Duration::from_secs(2)),
            None => {
                if process::exe_path(running.pid)
                    .is_some_and(|exe| same_file(&exe, &running.binary))
                {
                    process::terminate(running.pid, Duration::from_secs(2));
                }
            }
        }
        let crash = running.tail.lock().lines.clone();
        if !crash.is_empty() {
            log::warn!(
                "llama-server {} output before stop: {}",
                self.role.label(),
                crash.join(" | ")
            );
        }
        remove_marker_if(paths, self.port, running.pid);
        log::info!(
            "llama-server {} stopped pid={}",
            self.role.label(),
            running.pid
        );
    }

    /// Stops the server when it has been idle past its role's timeout.
    fn reap_if_idle(&self) {
        let mut state = self.state.lock();
        let idle = state.leases == 0
            && matches!(state.phase, Phase::Running(_))
            && state
                .idle_since
                .is_some_and(|since| since.elapsed() >= self.role.idle_timeout());
        if !idle {
            return;
        }
        let phase = std::mem::replace(&mut state.phase, Phase::Stopped);
        state.idle_since = None;
        drop(state);
        if let Phase::Running(running) = phase {
            log::info!("llama-server {} idle; unloading", self.role.label());
            self.stop_running(*running);
        }
        self.changed.notify_all();
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

// ------------------------------------------------------------------ manager

struct Shared {
    installer: Arc<Installer>,
    servers: [LlamaServer; 3],
    reaper: std::sync::Once,
}

impl Drop for Shared {
    fn drop(&mut self) {
        for server in &self.servers {
            server.stop();
        }
    }
}

/// The three role servers and the runtime they share. Cheap to clone and
/// `Send + Sync`; every clone controls the same servers.
#[derive(Clone)]
pub struct RuntimeManager {
    shared: Arc<Shared>,
}

impl RuntimeManager {
    /// Cheap: no download, no process start.
    pub fn new(paths: LibraryPaths, settings: &Settings) -> Self {
        let installer = Arc::new(Installer {
            paths,
            preference: Mutex::new(settings.llama_backend.clone()),
            active: Mutex::new(None),
            lock: Mutex::new(()),
        });
        let server = |role: Role| LlamaServer {
            inner: Arc::new(ServerInner {
                role,
                port: role.port(),
                installer: installer.clone(),
                state: Mutex::new(ServerState {
                    phase: Phase::Stopped,
                    leases: 0,
                    idle_since: None,
                    desired: None,
                }),
                changed: Condvar::new(),
            }),
        };
        let servers = [
            server(Role::Main),
            server(Role::Embedder),
            server(Role::Autocomplete),
        ];
        Self {
            shared: Arc::new(Shared {
                installer,
                servers,
                reaper: std::sync::Once::new(),
            }),
        }
    }

    pub fn paths(&self) -> &LibraryPaths {
        &self.shared.installer.paths
    }

    pub fn server(&self, role: Role) -> &LlamaServer {
        let index = Role::ALL.iter().position(|r| *r == role).unwrap_or(0);
        &self.shared.servers[index]
    }

    /// Applies a changed `llamaBackend` setting (`auto`, `vulkan`, `cpu`).
    /// Running servers keep their backend until they restart.
    pub fn set_backend_preference(&self, preference: &str) {
        let installer = &self.shared.installer;
        let mut current = installer.preference.lock();
        if current.trim() != preference.trim() {
            *current = preference.to_string();
            *installer.active.lock() = None;
        }
    }

    /// Downloads and installs the runtime the setting asks for (`auto` may
    /// fetch the Vulkan build to look for a GPU, then the CPU build).
    pub fn ensure_runtime(
        &self,
        settings: &Settings,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<()> {
        self.set_backend_preference(&settings.llama_backend);
        self.shared
            .installer
            .resolve(true, cancel, progress)
            .map(|_| ())
    }

    /// "Vulkan" or "CPU" when a runtime is installed: the one in use, else
    /// the one auto-detection chose, else whichever is present.
    pub fn installed_backend(&self) -> Option<String> {
        let installer = &self.shared.installer;
        let candidates = [
            *installer.active.lock(),
            installer.saved_choice().map(|c| c.backend),
        ];
        candidates
            .into_iter()
            .flatten()
            .chain([LlamaBackend::Vulkan, LlamaBackend::Cpu])
            .find(|backend| installer.record(*backend).is_some())
            .map(|backend| backend.label().to_string())
    }

    /// The installed `llama-server --version` line, if any.
    pub fn installed_version(&self) -> Option<String> {
        let installer = &self.shared.installer;
        [LlamaBackend::Vulkan, LlamaBackend::Cpu]
            .into_iter()
            .find_map(|b| installer.record(b).map(|(r, _)| r.version))
    }

    /// Stops every server and removes the downloaded runtimes.
    pub fn uninstall(&self) -> Result<()> {
        self.shutdown();
        let installer = &self.shared.installer;
        let _guard = installer.lock.lock();
        *installer.active.lock() = None;
        let root = installer.root();
        match std::fs::remove_dir_all(&root) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
                Err(err).with_context(|| format!("removing {}", root.display()))
            }
            _ => Ok(()),
        }
    }

    /// Stops all servers (app quit).
    pub fn shutdown(&self) {
        for server in &self.shared.servers {
            server.stop();
        }
    }

    pub fn statuses(&self) -> Vec<ServerStatus> {
        self.shared
            .servers
            .iter()
            .map(LlamaServer::status)
            .collect()
    }

    fn start_reaper(&self) {
        let weak: Weak<Shared> = Arc::downgrade(&self.shared);
        self.shared.reaper.call_once(|| {
            let spawned = std::thread::Builder::new()
                .name("lokalbot-llama-idle".into())
                .spawn(move || {
                    loop {
                        std::thread::sleep(Duration::from_secs(5));
                        let Some(shared) = weak.upgrade() else { return };
                        for server in &shared.servers {
                            server.inner.reap_if_idle();
                        }
                    }
                });
            if let Err(err) = spawned {
                log::warn!("could not start the idle-unload thread: {err}");
            }
        });
    }

    /// Leases the Think server with `model`.
    pub fn main_server(&self, model: &ModelEntry) -> Result<Lease> {
        self.lease(Role::Main, model)
    }

    /// Leases the embedder (EmbeddingGemma 2).
    pub fn embedder_server(&self) -> Result<Lease> {
        self.lease(Role::Embedder, catalog::embedding_entry())
    }

    pub fn autocomplete_server(&self, model: &ModelEntry) -> Result<Lease> {
        self.lease(Role::Autocomplete, model)
    }

    pub fn lease(&self, role: Role, model: &ModelEntry) -> Result<Lease> {
        self.start_reaper();
        self.server(role).lease_model(model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_and_arguments_follow_the_roles() {
        assert_eq!(Role::Main.release_port(), 17_872);
        assert_eq!(Role::Embedder.release_port(), 17_873);
        assert_eq!(Role::Autocomplete.release_port(), 17_874);
        let all: Vec<String> = CONDITIONAL_FLAGS.iter().map(|f| f.to_string()).collect();
        assert_eq!(
            Role::Main.extra_args(&all),
            ["--cache-ram", "2048", "--reasoning", "on"]
        );
        assert_eq!(Role::Main.extra_args(&[]), Vec::<String>::new());
        assert_eq!(
            Role::Embedder.extra_args(&[]),
            [
                "--embeddings",
                "-b",
                "2048",
                "-ub",
                "2048",
                "--parallel",
                "1"
            ]
        );
        assert_eq!(
            Role::Autocomplete.extra_args(&all),
            ["--parallel", "1", "--cache-ram", "512"]
        );
    }

    #[test]
    fn pinned_assets_exist_for_this_platform() {
        assert_eq!(assets().len(), 2);
        for asset in assets() {
            assert_eq!(asset.sha256.len(), 64);
            assert!(
                asset
                    .url()
                    .starts_with("https://github.com/ggml-org/llama.cpp/releases/download/b11474/")
            );
            assert!(asset.name.contains(asset.tag));
        }
    }

    #[test]
    fn help_flags_match_whole_options_only() {
        let help = "-rea,   --reasoning [on|off|auto]  use reasoning\n  --reasoning-budget N\n --cache-ram, -cram N\n --jinja\n";
        assert_eq!(
            supported_flags(help),
            ["--reasoning", "--cache-ram", "--jinja"]
        );
        assert_eq!(
            supported_flags("--reasoning-budget N\n--no-webui-x"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn software_vulkan_devices_are_not_gpus() {
        let output = "Available devices:\n  Vulkan0: llvmpipe (LLVM 17.0.6, 256 bits) (64000 MiB, 64000 MiB free)\n  Vulkan1: AMD Radeon RX 6600 XT (RADV NAVI23) (8176 MiB, 7900 MiB free)\n";
        assert_eq!(
            parse_gpu_devices(output),
            ["AMD Radeon RX 6600 XT (RADV NAVI23) (8176 MiB, 7900 MiB free)"]
        );
        assert!(parse_gpu_devices("Available devices:\n").is_empty());
    }

    #[test]
    fn output_tail_keeps_startup_lines_then_only_crashes() {
        let mut tail = OutputTail::default();
        tail.keep("0.00.077.284 E gguf_init_from_reader: failed to read magic\n");
        tail.keep("main: exiting");
        assert_eq!(
            tail.reason().as_deref(),
            Some("gguf_init_from_reader: failed to read magic")
        );
        tail.ready = true;
        tail.keep("request: {\"prompt\": \"private words\"}");
        tail.keep("ggml_assert failed");
        assert!(!tail.lines.iter().any(|l| l.contains("private")));
        assert_eq!(
            tail.lines.last().map(String::as_str),
            Some("ggml_assert failed")
        );
        for i in 0..20 {
            tail.keep(&format!("abort {i}"));
        }
        assert_eq!(tail.lines.len(), TAIL_LINES);
    }

    #[test]
    fn tokens_are_64_hex() {
        let token = new_token();
        assert_eq!(token.len(), 64);
        assert!(
            token
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_ne!(token, new_token());
    }

    #[test]
    fn markers_round_trip_and_stale_ones_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let paths = LibraryPaths::new(dir.path());
        let manager = RuntimeManager::new(paths.clone(), &Settings::default());
        let inner = &manager.server(Role::Main).inner;
        let marker = ServerMarker {
            pid: 4_000_000_000,
            port: inner.port,
            binary_path: "/nonexistent/llama-server".into(),
            model_path: "/nonexistent/m.gguf".into(),
            context_tokens: 32_768,
            extra_args: vec![],
            authentication_token: new_token(),
            owner_pid: Some(1),
        };
        write_marker(&paths, &marker);
        assert_eq!(read_marker(&paths, inner.port), Some(marker.clone()));
        let json = std::fs::read_to_string(marker_path(&paths, inner.port)).unwrap();
        assert!(json.contains("\"ownerPID\"") && json.contains("\"authenticationToken\""));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(marker_path(&paths, inner.port))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        // A dead helper's marker is discarded before binding.
        let reclaimed = inner
            .reclaim(
                "m",
                Path::new("/nonexistent/m.gguf"),
                Path::new("/nonexistent/llama-server"),
                32_768,
                &[],
                LlamaBackend::Cpu,
            )
            .unwrap();
        assert!(reclaimed.is_none());
        assert!(read_marker(&paths, inner.port).is_none());
    }

    #[test]
    fn leasing_a_missing_model_fails_without_downloading() {
        let dir = tempfile::tempdir().unwrap();
        let manager = RuntimeManager::new(LibraryPaths::new(dir.path()), &Settings::default());
        let err = manager
            .main_server(catalog::entry("qwen3.5-0.8b").unwrap())
            .err()
            .unwrap();
        assert!(
            matches!(
                err.downcast_ref::<AiError>(),
                Some(AiError::ModelMissing(_))
            ),
            "{err}"
        );
        assert!(!dir.path().join("runtimes").exists());
        assert_eq!(manager.installed_backend(), None);
        assert!(manager.statuses().iter().all(|s| !s.running));
    }
}
