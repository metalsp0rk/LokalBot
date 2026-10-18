//! Files the app carries for Agent Mode (`LokalBot/Resources/pi` on macOS):
//! the frozen pi manifest and lockfile, LokalBot's pi extension (provider
//! registration and the approval gate) and the meeting-library skill.
//!
//! The extension and skill are written into the runtime folder before each
//! launch from these copies, so a stale or edited file on disk never decides
//! what needs approval.

use std::path::Path;

use anyhow::{Context as _, Result};

use crate::layout::RuntimeLayout;

pub const PACKAGE_JSON: &str = include_str!("../resources/runtime/package.json");
pub const BUN_LOCK: &str = include_str!("../resources/runtime/bun.lock");
pub const EXTENSION_INDEX: &str = include_str!("../resources/lokalbot-extension/index.ts");
pub const SKILL: &str = include_str!("../resources/lokalbot-cli-skill/SKILL.md");

/// Writes `package.json` and `bun.lock` for `bun install`.
pub fn write_package_template(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    std::fs::write(dir.join("package.json"), PACKAGE_JSON)?;
    std::fs::write(dir.join("bun.lock"), BUN_LOCK)?;
    Ok(())
}

/// Makes the runtime's extension and skill folders hold exactly the built-in
/// copies. Returns the two folders.
pub fn materialize(layout: &RuntimeLayout) -> Result<(std::path::PathBuf, std::path::PathBuf)> {
    let extension = layout.extension_dir();
    let skill = layout.skill_dir();
    write_if_changed(&extension.join("index.ts"), EXTENSION_INDEX)?;
    write_if_changed(&skill.join("SKILL.md"), SKILL)?;
    Ok((extension, skill))
}

fn write_if_changed(path: &Path, content: &str) -> Result<()> {
    if std::fs::read(path).is_ok_and(|existing| existing == content.as_bytes()) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    // Temp file + rename: a pi starting in another task never reads half a file.
    lokalbot_core::library::write_atomic(path, content.as_bytes())
        .with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materialize_restores_edited_files() {
        let dir = tempfile::tempdir().unwrap();
        let layout = RuntimeLayout::new(dir.path());
        let (extension, skill) = materialize(&layout).unwrap();
        let index = extension.join("index.ts");
        assert_eq!(std::fs::read_to_string(&index).unwrap(), EXTENSION_INDEX);
        std::fs::write(&index, "export default function () {}").unwrap();
        materialize(&layout).unwrap();
        assert_eq!(std::fs::read_to_string(&index).unwrap(), EXTENSION_INDEX);
        assert!(skill.join("SKILL.md").is_file());
    }

    #[test]
    fn the_extension_gates_tools_and_registers_the_provider() {
        assert!(EXTENSION_INDEX.contains("pi.registerProvider(\"lokalbot\""));
        assert!(EXTENSION_INDEX.contains("lokalbot_tool_approval"));
        assert!(SKILL.contains("lokalbot-cli"));
    }
}
