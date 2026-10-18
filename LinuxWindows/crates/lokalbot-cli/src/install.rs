//! `install-skill` (`LokalBotCLIInstaller.swift`) for Linux and Windows.
//!
//! Linux: `~/.local/bin/lokalbot-cli` links to this binary; the skill is
//! written once under `~/.local/share/lokalbot-cli/skill` and linked from
//! `~/.agents/skills/lokalbot-cli` and `~/.claude/skills/lokalbot-cli`
//! (copied with `--copy`). Windows: the binary is copied to
//! `%LOCALAPPDATA%\Programs\LokalBot\bin` and the skill copied into the same
//! skill folders under `%USERPROFILE%`. Paths this installer does not own are
//! never replaced.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

/// The bundled agent skill (the macOS app ships the same file).
pub const SKILL: &str = include_str!("../../../../.agents/skills/lokalbot-cli/SKILL.md");
pub const COPY_MARKER: &str = ".lokalbot-skill-copy";
const PATH_EXPORT_LINE: &str = "export PATH=\"$HOME/.local/bin:$PATH\"";

pub struct Installer {
    pub home: PathBuf,
    /// The binary to install (this executable).
    pub binary: PathBuf,
    /// Where the installed command lives.
    pub bin_link: PathBuf,
    /// The skill source folder owned by the installer (link mode).
    pub skill_source: PathBuf,
    pub skill_link: PathBuf,
    pub claude_skill_link: PathBuf,
}

impl Installer {
    pub fn for_current_user() -> Result<Self> {
        let home = directories::BaseDirs::new()
            .map(|d| d.home_dir().to_path_buf())
            .context("could not find the home folder")?;
        let binary = std::env::current_exe()
            .and_then(|p| p.canonicalize())
            .context("could not find this executable")?;
        Ok(Self::new(home, binary))
    }

    pub fn new(home: PathBuf, binary: PathBuf) -> Self {
        let bin_link = if cfg!(windows) {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("AppData").join("Local"))
                .join("Programs")
                .join("LokalBot")
                .join("bin")
                .join("lokalbot-cli.exe")
        } else {
            home.join(".local").join("bin").join("lokalbot-cli")
        };
        Self {
            skill_source: home
                .join(".local")
                .join("share")
                .join("lokalbot-cli")
                .join("skill")
                .join("lokalbot-cli"),
            skill_link: home.join(".agents").join("skills").join("lokalbot-cli"),
            claude_skill_link: home.join(".claude").join("skills").join("lokalbot-cli"),
            bin_link,
            home,
            binary,
        }
    }

    /// The skill with this platform's paths in place of the macOS ones.
    pub fn skill_text(&self) -> String {
        SKILL
            .replace(
                "LokalBot is a private work-memory app for macOS.",
                "LokalBot is a private work-memory app for Linux and Windows.",
            )
            .replace(
                "/Applications/LokalBot.app/Contents/Helpers/lokalbot-cli",
                &self.bin_link.to_string_lossy(),
            )
    }

    pub fn bin_dir_on_path(&self) -> bool {
        let Some(dir) = self.bin_link.parent() else {
            return false;
        };
        std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|p| p == dir))
    }

    fn is_owned(&self, path: &Path) -> bool {
        if let Ok(target) = std::fs::read_link(path) {
            let name = target.file_name().and_then(|n| n.to_str()).unwrap_or("");
            return if path == self.bin_link {
                name == "lokalbot-cli" || name == "lokalbot-cli.exe"
            } else {
                target == self.skill_source
            };
        }
        if path == self.bin_link {
            // Windows installs a copy into a folder only this installer uses.
            return cfg!(windows) && path.is_file();
        }
        path.is_dir() && path.join(COPY_MARKER).is_file()
    }

    fn exists(path: &Path) -> bool {
        path.exists() || std::fs::symlink_metadata(path).is_ok()
    }

    fn require_owned_or_missing(&self, path: &Path) -> Result<()> {
        if Self::exists(path) && !self.is_owned(path) {
            bail!(
                "LokalBot did not replace {} because it is not owned by this installer. Move it to another location before installing.",
                path.display()
            );
        }
        Ok(())
    }

    fn remove(path: &Path) -> std::io::Result<()> {
        let metadata = std::fs::symlink_metadata(path)?;
        if metadata.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        }
    }

    fn write_skill_copy(&self, folder: &Path) -> Result<()> {
        std::fs::create_dir_all(folder)?;
        std::fs::write(folder.join("SKILL.md"), self.skill_text())?;
        std::fs::write(folder.join(COPY_MARKER), b"")?;
        Ok(())
    }

    pub fn install(&self, copy: bool) -> Result<()> {
        let destinations = [&self.bin_link, &self.skill_link, &self.claude_skill_link];
        for destination in destinations {
            self.require_owned_or_missing(destination)?;
        }
        for destination in destinations {
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Could not create {}", parent.display()))?;
            }
            if Self::exists(destination) {
                Self::remove(destination)
                    .with_context(|| format!("Could not replace {}", destination.display()))?;
            }
        }
        let link_mode = !copy && cfg!(unix);
        if cfg!(windows) {
            std::fs::copy(&self.binary, &self.bin_link)
                .with_context(|| format!("Could not install at {}", self.bin_link.display()))?;
        } else {
            symlink(&self.binary, &self.bin_link)?;
        }
        if link_mode {
            std::fs::create_dir_all(&self.skill_source)?;
            std::fs::write(self.skill_source.join("SKILL.md"), self.skill_text())?;
            symlink(&self.skill_source, &self.skill_link)?;
            symlink(&self.skill_source, &self.claude_skill_link)?;
        } else {
            self.write_skill_copy(&self.skill_link)?;
            self.write_skill_copy(&self.claude_skill_link)?;
        }
        Ok(())
    }

    /// Removes only what this installer created.
    pub fn uninstall(&self) -> Result<()> {
        for path in [&self.bin_link, &self.skill_link, &self.claude_skill_link] {
            if self.is_owned(path) {
                Self::remove(path)
                    .with_context(|| format!("Could not remove {}", path.display()))?;
            }
        }
        if self.skill_source.is_dir() {
            let _ = std::fs::remove_dir_all(&self.skill_source);
        }
        Ok(())
    }

    /// The text `install-skill` prints.
    pub fn installed_message(&self) -> String {
        let mut text = format!(
            "Installed:\n  {}\n  {}\n  {}\n\nClaude Code picks the skill up automatically; point other agents\nat one of the skill directories above.\n\nMCP clients (Claude Desktop, ...) can add the same library with:\n  claude mcp add lokalbot -- {} mcp",
            self.bin_link.display(),
            self.skill_link.display(),
            self.claude_skill_link.display(),
            self.bin_link.display()
        );
        if !self.bin_dir_on_path() {
            if cfg!(windows) {
                let dir = self
                    .bin_link
                    .parent()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                text.push_str(&format!(
                    "\n\nNote: {dir} isn't on your PATH. Add it in PowerShell with:\n  [Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path', 'User') + ';{dir}', 'User')"
                ));
            } else {
                let rc = if std::env::var("SHELL").is_ok_and(|s| s.ends_with("zsh")) {
                    "~/.zshrc"
                } else {
                    "~/.bashrc"
                };
                text.push_str(&format!(
                    "\n\nNote: ~/.local/bin isn't on your PATH. Add it with:\n  echo '{PATH_EXPORT_LINE}' >> {rc}"
                ));
            }
        }
        text
    }
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link)
        .with_context(|| format!("Could not link {}", link.display()))
}

#[cfg(not(unix))]
fn symlink(target: &Path, link: &Path) -> Result<()> {
    std::fs::copy(target, link)
        .with_context(|| format!("Could not install at {}", link.display()))?;
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn installs_links_and_refuses_foreign_paths() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let binary = dir.path().join("lokalbot-cli");
        std::fs::write(&binary, b"bin").unwrap();
        let installer = Installer::new(home.clone(), binary.clone());
        installer.install(false).unwrap();
        assert_eq!(std::fs::read_link(&installer.bin_link).unwrap(), binary);
        let skill = std::fs::read_to_string(installer.claude_skill_link.join("SKILL.md")).unwrap();
        assert!(skill.contains("Linux and Windows"));
        assert!(!skill.contains("/Applications/"));
        installer.install(true).unwrap();
        assert!(installer.skill_link.join(COPY_MARKER).is_file());
        installer.uninstall().unwrap();
        assert!(!Installer::exists(&installer.bin_link));
        assert!(!Installer::exists(&installer.skill_link));

        std::fs::create_dir_all(&installer.skill_link).unwrap();
        std::fs::write(installer.skill_link.join("SKILL.md"), "mine").unwrap();
        let err = installer.install(false).unwrap_err();
        assert!(
            err.to_string().contains("not owned by this installer"),
            "{err}"
        );
        assert!(
            installer
                .installed_message()
                .contains("claude mcp add lokalbot --")
        );
    }
}
