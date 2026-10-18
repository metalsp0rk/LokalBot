//! Real downloads and a real `llama-server`. Needs the network and ~1 GB of
//! disk; run on a development machine:
//!
//! ```sh
//! LOKALBOT_E2E_ROOT=~/scratch-root cargo test -p lokalbot-ai --test e2e_runtime -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use lokalbot_ai::catalog;
use lokalbot_ai::think::dot;
use lokalbot_ai::{ChatRequest, EmbeddingClient, RuntimeManager, ThinkClient, download};
use lokalbot_core::{LibraryPaths, Settings};

pub fn e2e_root() -> PathBuf {
    std::env::var_os("LOKALBOT_E2E_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("lokalbot-ai-e2e"))
}

fn progress(label: &'static str) -> impl FnMut(u64, u64) {
    let mut last = 0u64;
    move |done, total| {
        let step = total / 4;
        if total > 0 && (done == total || done >= last + step.max(1)) {
            last = done;
            eprintln!(
                "  {label}: {:.0}% of {:.1} MB",
                done as f64 * 100.0 / total as f64,
                total as f64 / 1e6
            );
        }
    }
}

#[test]
#[ignore = "downloads the llama.cpp runtime and two models"]
fn installs_runtime_downloads_models_and_answers() {
    let root = e2e_root();
    let paths = LibraryPaths::new(&root);
    let settings = Settings {
        built_in_model_id: catalog::COMPACT_FALLBACK_ID.into(),
        llama_backend: std::env::var("LOKALBOT_E2E_BACKEND").unwrap_or_else(|_| "auto".into()),
        ..Settings::default()
    };
    let cancel = AtomicBool::new(false);
    let runtime = RuntimeManager::new(paths.clone(), &settings);

    let started = Instant::now();
    runtime
        .ensure_runtime(&settings, &cancel, &mut progress("runtime"))
        .expect("runtime installs");
    eprintln!(
        "runtime ready in {:.1}s: backend={:?} version={:?}",
        started.elapsed().as_secs_f64(),
        runtime.installed_backend(),
        runtime.installed_version()
    );
    assert!(runtime.installed_backend().is_some());

    for id in [catalog::COMPACT_FALLBACK_ID, catalog::EMBEDDING_ID] {
        let entry = catalog::entry(id).unwrap();
        let started = Instant::now();
        download(
            &catalog::download_spec(&paths, entry),
            &cancel,
            &mut progress(entry.id),
        )
        .expect("model downloads");
        eprintln!("{id} ready in {:.1}s", started.elapsed().as_secs_f64());
        assert!(catalog::is_downloaded(&paths, entry));
        assert!(catalog::verify_downloaded(&paths, entry).unwrap());
    }

    let think = ThinkClient::new(&runtime, &settings).unwrap();
    let started = Instant::now();
    let hello = think
        .complete(
            &ChatRequest::new(
                "You are a connectivity test. Reply with one short sentence.",
                "Say hello and name the model you are.",
            )
            .max_tokens(96)
            .reasoning_budget(0)
            .temperature(0.2),
        )
        .expect("chat works");
    eprintln!(
        "first reply (cold start) in {:.1}s: {hello:?}",
        started.elapsed().as_secs_f64()
    );
    assert!(!hello.trim().is_empty());
    assert!(think.token_count("Hello there").unwrap().unwrap() >= 2);

    #[derive(serde::Deserialize, Debug)]
    struct Colors {
        colors: Vec<String>,
    }
    let schema = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["colors"],
        "properties": {"colors": {"type": "array", "maxItems": 3, "items": {"type": "string", "enum": ["red", "green", "blue", "black"]}}}
    });
    let started = Instant::now();
    let colors: Colors = think
        .complete_json(
            &ChatRequest::new("Return only JSON.", "Name the colors of a traffic light.")
                .max_tokens(128)
                .reasoning_budget(0)
                .temperature(0.0)
                .json_schema(schema),
        )
        .expect("schema-constrained JSON");
    eprintln!(
        "json reply in {:.1}s: {colors:?}",
        started.elapsed().as_secs_f64()
    );
    assert!(!colors.colors.is_empty());

    let mut streamed = String::new();
    let text = think
        .stream(
            &ChatRequest::new("Answer briefly.", "Count from one to five in words.")
                .max_tokens(64)
                .reasoning_budget(0),
            &mut |d| streamed.push_str(d),
        )
        .expect("streaming works");
    eprintln!("streamed: {text:?}");
    assert_eq!(text.trim(), streamed.trim());

    let embedder = EmbeddingClient::new(&runtime);
    let started = Instant::now();
    let documents = embedder
        .embed_documents(&[
            "Redis caching layer review and failover benchmark".into(),
            "Lunch menu for Friday".into(),
        ])
        .unwrap();
    let query = embedder.embed_query("cache failover benchmark").unwrap();
    eprintln!(
        "embeddings in {:.1}s: dims={} related={:.3} unrelated={:.3}",
        started.elapsed().as_secs_f64(),
        query.len(),
        dot(&query, &documents[0]),
        dot(&query, &documents[1])
    );
    assert_eq!(query.len(), catalog::EMBEDDING_DIMENSIONS);
    assert!(dot(&query, &documents[0]) > dot(&query, &documents[1]));
    for status in runtime.statuses() {
        eprintln!(
            "server {:?}: running={} model={:?} backend={:?}",
            status.role, status.running, status.model_id, status.backend
        );
    }
    runtime.shutdown();
    assert!(runtime.statuses().iter().all(|s| !s.running));
}
