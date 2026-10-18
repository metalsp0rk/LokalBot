//! Embeds the app icon, version details and the DPI-aware manifest into the
//! Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_resources();
    }
}

#[cfg(windows)]
fn windows_resources() {
    let packaging = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging");
    let icon = packaging.join("icons/lokalbot.ico");
    let manifest = packaging.join("windows/lokalbot.exe.manifest");
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-changed={}", manifest.display());
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon(&icon.to_string_lossy())
        .set_manifest_file(&manifest.to_string_lossy())
        .set("ProductName", "LokalBot")
        .set("FileDescription", "LokalBot")
        .set("CompanyName", "LokalBot")
        .set("LegalCopyright", "GPL-3.0-only");
    if let Err(err) = resource.compile() {
        panic!("embedding Windows resources failed: {err}");
    }
}

#[cfg(not(windows))]
fn windows_resources() {
    println!("cargo:warning=Windows resources can only be embedded when building on Windows");
}
