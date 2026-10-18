//! Runs one recording through the real pipeline without the UI: transcription,
//! speaker attribution, notes and actions, indexing.
//!
//! `cargo run -p lokalbot-engine --example process_meeting -- <library> <mic.wav> [system.wav]`
//!
//! The library must already hold the models its `settings.json` selects.

use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, bail};
use lokalbot_core::{Library, LibraryPaths, Settings};
use lokalbot_engine::{Command, Engine, EngineEvent, Stage};

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args: Vec<String> = std::env::args().collect();
    let (Some(root), Some(mic)) = (args.get(1), args.get(2)) else {
        bail!("usage: process_meeting <library> <mic.wav> [system.wav]");
    };
    let system = args.get(3);
    let paths = LibraryPaths::new(root);
    let library = Library::open(paths.clone())?;
    let settings = Settings::load(&paths.settings_file());

    let started = lokalbot_core::time::now_seconds() - chrono::Duration::minutes(5);
    let mut meeting = library.create_meeting("Pipeline check", "Test call", started)?;
    let dir = library.meeting_dir(&meeting);
    std::fs::copy(mic, dir.join("mic.wav")).context("copying the mic track")?;
    if let Some(system) = system {
        std::fs::copy(system, dir.join("system.wav")).context("copying the system track")?;
        meeting.has_system_track = true;
    }
    meeting.ended_at = Some(lokalbot_core::time::now_seconds());
    library.save_meeting(&meeting)?;

    let (sender, receiver) = crossbeam_channel::unbounded();
    let engine = Engine::start(library.clone(), settings, move |event| {
        let _ = sender.send(event);
    })?;
    let clock = Instant::now();
    engine.send(Command::ProcessMeeting {
        meeting_id: meeting.id,
        transcribe: true,
        summarize: true,
    });
    let deadline = Duration::from_secs(45 * 60);
    loop {
        let Ok(event) = receiver.recv_timeout(deadline.saturating_sub(clock.elapsed())) else {
            bail!("timed out after {:?}", clock.elapsed());
        };
        match event {
            EngineEvent::Processing { meeting_id, stage } if meeting_id == meeting.id => {
                match stage {
                    Some(Stage::Failed(message)) => bail!("processing failed: {message}"),
                    Some(Stage::WaitingForModels) => {
                        bail!("the library is missing models for these settings")
                    }
                    Some(stage) => println!("[{:>6.1}s] {stage:?}", clock.elapsed().as_secs_f64()),
                    None => break,
                }
            }
            EngineEvent::Message {
                text,
                is_error: true,
            } => eprintln!("error: {text}"),
            _ => {}
        }
    }
    println!("done in {:.1}s\n", clock.elapsed().as_secs_f64());
    let transcript = library.transcript(&meeting)?.unwrap_or_default();
    println!(
        "=== transcript ({} segments, engine {})\n{}\n",
        transcript.segments.len(),
        transcript.engine,
        transcript.markdown()
    );
    println!(
        "=== summary.md\n{}",
        library.summary(&meeting)?.unwrap_or_default()
    );
    let outcomes = library.outcomes(&meeting)?.unwrap_or_default();
    println!(
        "=== outcomes: {} actions, {} decisions, {} open questions",
        outcomes.action_items.len(),
        outcomes.decision_records.len(),
        outcomes.open_questions.len()
    );
    drop(engine);
    Ok(())
}
