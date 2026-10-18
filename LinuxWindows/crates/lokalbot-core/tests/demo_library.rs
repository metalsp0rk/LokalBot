//! The demo library exercised through the public API the app uses.

use chrono::{Local, Utc};
use lokalbot_core::{ActionCorrection, ActionStatus, Library, LibraryPaths, TaskStatus, demo};

fn seeded() -> (tempfile::TempDir, Library) {
    let dir = tempfile::tempdir().unwrap();
    let paths = LibraryPaths::new(dir.path().join("library"));
    demo::seed(&paths, Utc::now()).unwrap();
    let library = Library::open(paths).unwrap();
    (dir, library)
}

#[test]
fn today_is_populated() {
    let (_dir, library) = seeded();
    let today = Local::now().date_naive();
    let day = library.day_activity(today).unwrap();
    assert_eq!(day.meeting_count, 3, "{day:#?}");
    assert_eq!(day.moment_count, 6);
    assert!(
        day.tracked_seconds > 2.0 * 3600.0,
        "tracked {}",
        day.tracked_seconds
    );
    assert!(day.sessions.iter().any(|s| s.meeting_id.is_some()));
    assert!(day.sessions.iter().any(|s| s.meeting_id.is_none()));
    assert!(!day.app_usage.is_empty());
    assert!(
        day.app_usage
            .windows(2)
            .all(|w| w[0].seconds >= w[1].seconds)
    );

    let digest = library.day_digest(today).unwrap().expect("today's digest");
    assert_eq!(digest.highlights.len(), 3);
    assert_eq!(digest.tasks.len(), 4);
    assert_eq!(digest.tasks[3].status, TaskStatus::Completed);
    assert!(digest.tasks[0].next_step.is_some());
    assert!(digest.meta.is_some());
    assert_eq!(digest.decisions.len(), 2);
    assert_eq!(digest.blockers.len(), 1);
    assert!(digest.meetings_markdown.is_some());
    assert!(!digest.time_allocations.is_empty());

    let moments = library
        .screen_moments(
            today.and_hms_opt(0, 0, 0).unwrap().and_utc() - chrono::Duration::days(1),
            Utc::now() + chrono::Duration::days(1),
        )
        .unwrap();
    assert!(moments.iter().filter(|m| m.is_saved).count() == 1);
    assert!(moments.iter().all(|m| m.text.is_some()));
}

#[test]
fn needs_attention_puts_overdue_work_first() {
    let (_dir, library) = seeded();
    let threads = library.needs_attention(None, 3).unwrap();
    assert_eq!(threads.len(), 3);
    assert!(
        threads
            .iter()
            .all(|t| t.is_for_user() && t.status() == ActionStatus::Open)
    );
    let today = Local::now().date_naive();
    assert!(
        threads[0].is_overdue(today),
        "{:?}",
        threads.iter().map(|t| &t.text).collect::<Vec<_>>()
    );
    let all = library.action_threads(None).unwrap();
    let captioned = all
        .iter()
        .find(|t| t.text == "Deliver the captioned Northwind cut")
        .unwrap();
    assert_eq!(captioned.meeting_count(), 2);
    assert!(all.iter().any(|t| t.status() == ActionStatus::Done));
    assert!(all.iter().any(|t| t.status() == ActionStatus::Deferred));
    assert!(all.iter().any(|t| !t.is_for_user()));
}

#[test]
fn corrections_update_state_and_summary() {
    let (_dir, library) = seeded();
    let meeting = library
        .list_meetings()
        .unwrap()
        .into_iter()
        .find(|m| m.title == "Holiday shoot planning")
        .unwrap();
    let actions = library.meeting_actions(&meeting).unwrap();
    let book = actions
        .iter()
        .find(|a| a.text.starts_with("Book the photo studio"))
        .unwrap();
    library
        .correct_action(
            &meeting,
            &book.action_id,
            ActionCorrection {
                text: Some("Book the photo studio and the softbox".into()),
                owner: Some(Some("Maya Chen".into())),
                due: Some(None),
            },
        )
        .unwrap();
    let corrected = library.meeting_actions(&meeting).unwrap();
    let book = corrected
        .iter()
        .find(|a| a.action_id == book.action_id)
        .unwrap();
    assert_eq!(book.text, "Book the photo studio and the softbox");
    assert_eq!(book.owner.as_deref(), Some("Maya Chen"));
    assert!(!book.is_for_user);
    assert_eq!(book.due, None);
    let summary = library.summary(&meeting).unwrap().unwrap();
    assert!(
        summary.contains("- [ ] Maya Chen: Book the photo studio and the softbox"),
        "{summary}"
    );
    library
        .set_action_status(&meeting, &book.action_id, ActionStatus::Done)
        .unwrap();
    assert_eq!(
        library
            .meeting_actions(&meeting)
            .unwrap()
            .iter()
            .find(|a| a.action_id == book.action_id)
            .unwrap()
            .status,
        ActionStatus::Done
    );
}

#[test]
fn macos_fixture_actions_project() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../LokalBotTests/Fixtures/upgrade/0.10.9/library");
    if !root.is_dir() {
        return;
    }
    let library = Library::open_read_only(LibraryPaths::new(root)).unwrap();
    let references = library.action_references(None).unwrap();
    assert!(references.len() >= 10, "{}", references.len());
    assert!(references.iter().any(|r| r.is_for_user));
    let threads = library.action_threads(None).unwrap();
    assert!(!threads.is_empty());
    let digest = library
        .day_digest(chrono::NaiveDate::from_ymd_opt(2026, 10, 17).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(digest.tasks.len(), 3);
    assert_eq!(digest.highlights.len(), 1);
}

/// `LB_DEMO_OUT=/tmp/x cargo test -p lokalbot-core --test demo_library -- --ignored`
#[test]
#[ignore]
fn write_demo_library() {
    let out = std::env::var("LB_DEMO_OUT").expect("LB_DEMO_OUT");
    demo::seed(&LibraryPaths::new(out), Utc::now()).unwrap();
}
