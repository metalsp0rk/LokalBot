//! API keys live in the system credential store (Windows Credential Manager,
//! the Secret Service on Linux), never in `settings.json`.

use anyhow::{Context as _, Result};

const SERVICE: &str = "com.lokalbot.LokalBot";
const OPENAI_COMPATIBLE: &str = "openai-compatible-api-key";

fn entry(account: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, account).context("the system credential store is not available")
}

/// Saves (or, when blank, removes) the key for the OpenAI-compatible server.
pub fn save_openai_api_key(key: &str) -> Result<()> {
    let entry = entry(OPENAI_COMPATIBLE)?;
    if key.trim().is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(err).context("removing the API key"),
        }
    } else {
        entry.set_password(key.trim()).context("saving the API key")
    }
}

pub fn openai_api_key() -> Option<String> {
    entry(OPENAI_COMPATIBLE)
        .ok()?
        .get_password()
        .ok()
        .filter(|k| !k.trim().is_empty())
}
