//! Think, Autocomplete and embeddings for LokalBot on Linux and Windows.
//!
//! - [`catalog`]: the pinned GGUF models (same files and digests as macOS).
//! - [`download`]: verified, resumable HTTPS downloads and archive extraction.
//! - [`runtime`]: prebuilt llama.cpp `llama-server` subprocesses, one per role.
//! - [`think`]: chat, embedding and raw-completion clients with the consent gate.
//!
//! Errors are `anyhow::Error`; the ones callers act on are [`AiError`] values
//! inside it (`err.downcast_ref::<AiError>()`).

pub mod catalog;
pub mod download;
mod error;
mod http;
pub mod json;
mod process;
pub mod runtime;
pub mod think;

pub use catalog::{ModelEntry, ModelKind, Preset};
pub use download::{DownloadSpec, download, extract_archive};
pub use error::{AiError, ai_error};
pub use runtime::{Lease, LlamaBackend, LlamaServer, Role, RuntimeManager};
pub use think::{
    ChatRequest, Completion, CompletionClient, CompletionRequest, EmbeddingClient, ThinkClient,
};
