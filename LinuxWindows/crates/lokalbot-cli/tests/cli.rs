//! The real binary against a seeded demo library and the macOS fixture.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use lokalbot_core::{LibraryPaths, access, demo};

fn seeded() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("library");
    demo::seed(&LibraryPaths::new(&root), chrono::Utc::now()).unwrap();
    (dir, root)
}

fn cli(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lokalbot-cli"))
        .args(args)
        .env("LOKALBOT_STORAGE_ROOT", root)
        .env_remove(access::CAPABILITY_ENV)
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Every file under `meetings/` with its size and modification time.
fn snapshot(root: &Path) -> Vec<(PathBuf, u64, std::time::SystemTime)> {
    let mut out = Vec::new();
    let mut stack = vec![root.join("meetings")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            let metadata = entry.metadata().unwrap();
            if metadata.is_dir() {
                stack.push(path);
            } else {
                out.push((path, metadata.len(), metadata.modified().unwrap()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn commands_honor_the_access_gate() {
    let (_dir, root) = seeded();
    let output = cli(&root, &["list", "--table"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("External agent access is off"));
    let brief = cli(&root, &["brief"]);
    assert!(brief.status.success());
    assert!(stdout(&brief).contains("- Meetings: off."));
}

#[test]
fn list_get_search_actions_and_people() {
    let (_dir, root) = seeded();
    access::set_meetings_access(&LibraryPaths::new(&root), true).unwrap();
    let before = snapshot(&root);

    let table = stdout(&cli(&root, &["list", "--table"]));
    assert!(table.starts_with("ID        Date"), "{table}");
    assert!(table.contains("Holiday shoot planning"));
    assert_eq!(table.lines().count(), 2 + 17);

    let json: serde_json::Value =
        serde_json::from_str(&stdout(&cli(&root, &["get", "latest", "--format", "json"]))).unwrap();
    assert_eq!(json["uuid"], "55555555-5555-4555-8555-555555555555");
    assert_eq!(json["id"], "55555555");
    assert!(json["summary"].as_str().unwrap().contains("## TL;DR"));
    assert!(
        json["transcript"]
            .as_str()
            .unwrap()
            .contains("**[00:00:12] Maya Chen:**")
    );
    assert!(
        json["folder"]
            .as_str()
            .unwrap()
            .ends_with("holiday-shoot-planning")
    );

    let markdown = stdout(&cli(
        &root,
        &["get", "5555", "--include", "metadata,transcript"],
    ));
    assert!(markdown.starts_with("# Holiday shoot planning\n**Date:** "));
    assert!(markdown.contains("## Transcript") && !markdown.contains("## Summary"));

    let hits: serde_json::Value =
        serde_json::from_str(&stdout(&cli(&root, &["search", "softbox"]))).unwrap();
    let hits = hits.as_array().unwrap();
    assert!(!hits.is_empty());
    assert!(
        hits.iter()
            .all(|h| h["meeting_id"].as_str().unwrap().len() == 8)
    );
    assert!(
        hits.iter()
            .any(|h| h["match_kind"] == "transcript" && h["timestamp"].is_string())
    );

    let quoted: serde_json::Value =
        serde_json::from_str(&stdout(&cli(&root, &["search", "\"photo studio\""]))).unwrap();
    assert!(quoted.as_array().unwrap().iter().all(|h| {
        h["snippet"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("photo studio")
    }));

    let actions: serde_json::Value = serde_json::from_str(&stdout(&cli(
        &root,
        &["actions", "--owner", "me", "--days", "30"],
    )))
    .unwrap();
    let first = &actions.as_array().unwrap()[0];
    assert_eq!(first["overdue"], true);
    assert_eq!(first["owner"], "Me");

    let people = stdout(&cli(&root, &["people", "--table"]));
    assert!(people.contains("Maya Chen") && people.contains("John Park"));
    let maya: serde_json::Value =
        serde_json::from_str(&stdout(&cli(&root, &["people", "Maya"]))).unwrap();
    assert_eq!(maya["name"], "Maya Chen");
    assert!(maya["meetings"].as_array().unwrap().len() >= 5);

    let bad = cli(&root, &["get", "latest", "--include", "audio"]);
    assert_eq!(bad.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("Unknown --include value(s): audio."));
    let path = stdout(&cli(&root, &["path"]));
    assert_eq!(path.trim(), root.display().to_string());

    assert_eq!(
        before,
        snapshot(&root),
        "the CLI must never write into meetings/"
    );
}

#[test]
fn mcp_round_trip() {
    let (_dir, root) = seeded();
    access::set_meetings_access(&LibraryPaths::new(&root), true).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lokalbot-cli"))
        .arg("mcp")
        .env("LOKALBOT_STORAGE_ROOT", &root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let requests = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2099-01-01"}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search_meetings","arguments":{"query":"captioned","limit":3}}}"#,
        r#"{"jsonrpc":"2.0","id":"x","method":"tools/call","params":{"name":"get_timeline","arguments":{}}}"#,
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"list_meetings","arguments":{"limit":0}}}"#,
        "",
        r#"{"jsonrpc":"2.0","id":6,"method":"ping"}"#,
    ];
    {
        let stdin = child.stdin.as_mut().unwrap();
        for request in requests {
            writeln!(stdin, "{request}").unwrap();
        }
    }
    let output = child.wait_with_output().unwrap();
    let lines: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 6, "notifications and blank lines get no reply");
    assert_eq!(lines[0]["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(lines[0]["result"]["serverInfo"]["name"], "lokalbot");
    let tools = lines[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 14);
    assert_eq!(tools[0]["name"], "get_briefing");
    assert_eq!(tools[13]["name"], "get_personal_context");
    assert_eq!(lines[2]["result"]["isError"], false);
    let hits: serde_json::Value =
        serde_json::from_str(lines[2]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(!hits.as_array().unwrap().is_empty());
    assert_eq!(lines[3]["id"], "x");
    assert_eq!(lines[3]["result"]["isError"], true);
    assert!(
        lines[3]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("[screen_access_disabled] ")
    );
    assert_eq!(
        lines[4]["result"]["content"][0]["text"],
        "[invalid_arguments] \"limit\" must be an integer between 1 and 1000."
    );
    assert_eq!(lines[5]["result"], serde_json::json!({}));
    assert!(String::from_utf8_lossy(&output.stderr).contains("lokalbot-cli mcp: serving on stdio"));
}

#[test]
fn version_exits_without_touching_the_library() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("library");
    for flag in ["--version", "-V"] {
        let output = cli(&root, &[flag]);
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(
            stdout(&output),
            format!("lokalbot-cli {}\n", env!("CARGO_PKG_VERSION"))
        );
    }
    assert!(!root.exists());
}

#[test]
fn reads_the_macos_fixture() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../LokalBotTests/Fixtures/upgrade/0.10.9/library");
    if !fixture.is_dir() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("library");
    copy_dir(&fixture, &root);
    access::set_meetings_access(&LibraryPaths::new(&root), true).unwrap();
    let list: serde_json::Value = serde_json::from_str(&stdout(&cli(&root, &["list"]))).unwrap();
    assert!(list.as_array().unwrap().len() >= 10);
    let hits: serde_json::Value =
        serde_json::from_str(&stdout(&cli(&root, &["search", "Redis"]))).unwrap();
    assert!(!hits.as_array().unwrap().is_empty());
    let get = stdout(&cli(&root, &["get", "44444444", "--include", "summary"]));
    assert!(get.contains("## Summary"));
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}
