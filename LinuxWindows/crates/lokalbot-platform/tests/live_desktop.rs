//! Checks against a real desktop session, ignored by default. Run them with a
//! display, for example `scripts/live-x11.sh` style: Xvfb, a window manager
//! and a titled window, then
//! `LOKALBOT_EXPECT_TITLE=… cargo test -p lokalbot-platform --test live_desktop -- --ignored`.

#![cfg(target_os = "linux")]

use std::process::Command;
use std::time::Duration;

use lokalbot_platform::{
    GlobalShortcuts, SampleDisposition, Shortcut, ShortcutError, activity_source, disposition,
};

#[test]
#[ignore]
fn x11_sampler_reads_the_active_window_and_idle_time() {
    let expected = std::env::var("LOKALBOT_EXPECT_TITLE").expect("LOKALBOT_EXPECT_TITLE");
    let mut source = activity_source();
    assert_eq!(source.backend_name(), "x11", "{:?}", source.limitation());
    assert_eq!(source.limitation(), None);
    let sample = source.sample().unwrap().expect("a focused window");
    println!("sample: {sample:?}");
    assert_eq!(sample.title, expected);
    assert!(!sample.app.is_empty() && !sample.app_id.is_empty());

    let stored = disposition(&sample, &lokalbot_core::Settings::default());
    println!("stored: {stored:?}");
    let SampleDisposition::Keep(kept) = stored else {
        panic!("an ordinary window is kept");
    };
    assert!(!kept.title.contains("abcdef123456"), "secrets are redacted");

    // Input resets the idle clock; quiet time grows it.
    Command::new("xdotool")
        .args(["mousemove", "10", "10"])
        .status()
        .unwrap();
    std::thread::sleep(Duration::from_millis(200));
    let fresh = source.idle_seconds().expect("idle time");
    std::thread::sleep(Duration::from_secs(2));
    let later = source.idle_seconds().expect("idle time");
    println!("idle: {fresh:.2}s then {later:.2}s");
    assert!(fresh < 1.5, "{fresh}");
    assert!(later >= fresh + 1.5, "{fresh} → {later}");
}

#[test]
#[ignore]
fn x11_shortcuts_report_press_and_release() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut shortcuts = GlobalShortcuts::new(move |event| {
        let _ = tx.send(event);
    })
    .unwrap();
    assert_eq!(shortcuts.backend_name(), "x11");
    let chord = Shortcut::parse("Ctrl+Alt+K").unwrap();
    shortcuts.register("dictation", &chord).unwrap();
    assert!(matches!(
        shortcuts.register("ask", &chord),
        Err(ShortcutError::InUse)
    ));

    Command::new("xdotool")
        .args(["keydown", "ctrl+alt+k"])
        .status()
        .unwrap();
    let down = rx.recv_timeout(Duration::from_secs(3)).expect("press");
    std::thread::sleep(Duration::from_millis(300));
    Command::new("xdotool")
        .args(["keyup", "ctrl+alt+k"])
        .status()
        .unwrap();
    let up = rx.recv_timeout(Duration::from_secs(3)).expect("release");
    println!("events: {down:?} {up:?}");
    assert_eq!((down.id.as_str(), down.pressed), ("dictation", true));
    assert_eq!((up.id.as_str(), up.pressed), ("dictation", false));

    shortcuts.unregister("dictation").unwrap();
    Command::new("xdotool")
        .args(["key", "ctrl+alt+k"])
        .status()
        .unwrap();
    assert!(rx.recv_timeout(Duration::from_millis(700)).is_err());
}
