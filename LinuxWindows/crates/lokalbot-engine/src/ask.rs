//! Answering questions in Ask, and writing project notes.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use chrono::Utc;
use uuid::Uuid;

use lokalbot_core::{ChatMessage, ChatRole, Conversation};

use crate::models::Models;
use crate::{AskScope, AskUpdate, EngineEvent, Shared};

pub struct AskWorker {
    shared: Arc<Shared>,
    models: Models,
    running: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
}

impl AskWorker {
    pub fn new(shared: Arc<Shared>, models: Models) -> Self {
        Self {
            shared,
            models,
            running: Arc::new(AtomicBool::new(false)),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn ask(&self, conversation_id: Uuid, question: String, scope: AskScope) {
        if self.running.swap(true, Ordering::SeqCst) {
            self.shared
                .message("LokalBot is still answering the previous question.");
            return;
        }
        self.cancel.store(false, Ordering::Relaxed);
        let shared = self.shared.clone();
        let models = self.models.clone();
        let running = self.running.clone();
        let cancel = self.cancel.clone();
        std::thread::spawn(move || {
            let result = answer(
                &shared,
                &models,
                conversation_id,
                &question,
                &scope,
                &cancel,
            );
            running.store(false, Ordering::SeqCst);
            match result {
                Ok(()) => shared.emit(EngineEvent::Ask(AskUpdate::Finished { conversation_id })),
                Err(err) => shared.emit(EngineEvent::Ask(AskUpdate::Failed {
                    conversation_id,
                    message: format!("{err:#}"),
                })),
            }
            shared.emit(EngineEvent::LibraryChanged);
        });
    }

    pub fn write_project_note(&self, project_id: String) {
        let shared = self.shared.clone();
        let models = self.models.clone();
        std::thread::spawn(move || {
            shared.emit(EngineEvent::ProjectNote {
                project_id: project_id.clone(),
                running: true,
            });
            let result = (|| -> Result<()> {
                if !models.missing(&models.think_requirements()).is_empty() {
                    return Err(anyhow!(
                        "Download the Think model in Settings → Models first."
                    ));
                }
                let settings = shared.settings();
                let think = shared.think(&settings)?;
                let projects = shared.library.projects()?;
                let project = projects
                    .into_iter()
                    .find(|p| p.id == project_id)
                    .ok_or_else(|| anyhow!("That project no longer exists."))?;
                let summary = crate::compat::project_note(&think, &shared.library, &project)?;
                shared.library.save_project_summary(summary)?;
                Ok(())
            })();
            if let Err(err) = result {
                shared.error(format!("Could not write the project note: {err:#}"));
            }
            shared.emit(EngineEvent::ProjectNote {
                project_id,
                running: false,
            });
            shared.emit(EngineEvent::LibraryChanged);
        });
    }
}

fn answer(
    shared: &Shared,
    models: &Models,
    conversation_id: Uuid,
    question: &str,
    scope: &AskScope,
    cancel: &AtomicBool,
) -> Result<()> {
    let library = &shared.library;
    let settings = shared.settings();
    let progress = |text: &str, activity: &[String], status: Option<String>| {
        shared.emit(EngineEvent::Ask(AskUpdate::Progress {
            conversation_id,
            question: question.to_string(),
            text: text.to_string(),
            activity: activity.to_vec(),
            status,
        }));
    };
    if !models.missing(&models.think_requirements()).is_empty() {
        return Err(anyhow!(
            "Ask needs the Think model. Download it in Settings → Models, then ask again."
        ));
    }
    progress("", &[], Some("Preparing the assistant".into()));
    let think = shared.think(&settings)?;

    let mut conversation = library
        .conversations()?
        .into_iter()
        .find(|c| c.id == conversation_id)
        .unwrap_or_else(|| Conversation {
            id: conversation_id,
            title: title_for(question),
            created_at: lokalbot_core::time::now_seconds(),
            updated_at: lokalbot_core::time::now_seconds(),
            messages: Vec::new(),
        });
    let history = conversation.messages.clone();

    let mut text = String::new();
    let mut activity: Vec<String> = Vec::new();
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    let reply = crate::compat::answer(
        &think,
        library,
        question,
        &history,
        scope,
        cancel,
        &mut |event| {
            match event {
                crate::compat::AskEvent::Delta(delta) => text.push_str(&delta),
                crate::compat::AskEvent::Activity(step) => activity.push(step),
                crate::compat::AskEvent::Status(status) => progress(&text, &activity, Some(status)),
            }
            if last_emit.elapsed() > Duration::from_millis(60) {
                last_emit = Instant::now();
                progress(&text, &activity, None);
            }
        },
    )?;

    let now = Utc::now();
    conversation.messages.push(ChatMessage {
        id: Uuid::new_v4(),
        role: ChatRole::User,
        text: question.to_string(),
        activity: Vec::new(),
        created_at: Some(lokalbot_core::time::now_seconds()),
    });
    let mut reply = reply;
    if cancel.load(Ordering::Relaxed) && !reply.text.is_empty() {
        reply.text.push_str("\n\n_Response stopped._");
    }
    conversation.messages.push(reply);
    conversation.updated_at = lokalbot_core::time::now_seconds();
    let _ = now;
    library.save_conversation(&conversation)?;
    Ok(())
}

fn title_for(question: &str) -> String {
    let trimmed = question.trim();
    if trimmed.chars().count() <= 60 {
        return trimmed.to_string();
    }
    let mut title: String = trimmed.chars().take(57).collect();
    if let Some(space) = title.rfind(' ') {
        title.truncate(space);
    }
    title.push('…');
    title
}
