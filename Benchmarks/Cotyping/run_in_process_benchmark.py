#!/usr/bin/env python3
"""Repeat the real headless engine suite with synthetic prompts and isolated storage.

No UI automation, installed-app preferences, clipboard, or writing history.
The supplied app must be a local build supporting --cotyping-bench.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import subprocess
import tempfile
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", required=True, type=Path, help="Path to the built executable")
    parser.add_argument("--model", required=True, type=Path, help="Existing catalog LFM2.5 or Gemma E2B Base GGUF")
    parser.add_argument("--output", required=True, type=Path, help="New result directory")
    parser.add_argument("--repetitions", type=int, default=3)
    args = parser.parse_args()
    app = args.app.resolve(strict=True)
    model = args.model.resolve(strict=True)
    if args.repetitions < 1:
        parser.error("--repetitions must be positive")
    model_ids = {"LFM2.5-1.2B-Instruct-Q4_K_M.gguf": "lfm2.5-1.2b-instruct",
                 "gemma-4-E2B.i1-Q6_K.gguf": "gemma4-e2b-base-q6"}
    if model.name not in model_ids:
        parser.error("Use a qualified LFM2.5 Instruct or Gemma E2B Base catalog model")
    args.output.mkdir(parents=True, exist_ok=False)
    settings = {
        "cotypingBuiltInModelID": model_ids[model.name],
        "cotypingInProcessRuntime": True,
        "cotypingEnabled": False,
        "cotypingMaxWords": 3,
        "cotypingUseLocalLearning": False,
        "cotypingUseClipboard": False,
        "cotypingUseAppContext": True,
    }
    with app.open("rb") as source:
        app_sha = hashlib.file_digest(source, "sha256").hexdigest()
    with model.open("rb") as source:
        model_sha = hashlib.file_digest(source, "sha256").hexdigest()
    manifest = {
        "appSHA256": app_sha,
        "modelFile": model.name,
        "modelSHA256": model_sha,
        "settings": settings,
        "repetitions": args.repetitions,
        "boundary": "Engine only; excludes input monitoring, debounce, AX validation, and overlay",
    }
    (args.output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    for index in range(args.repetitions):
        with tempfile.TemporaryDirectory(prefix="lokalbot-cotyping-bench-") as temp:
            root = Path(temp)
            suite = "me.dotenv.LokalBot.cotyping-bench." + uuid.uuid4().hex
            preferences = root / "home/Library/Preferences"
            preferences.mkdir(parents=True)
            fixture = plistlib.dumps({
                "lokalbotv3.settings": json.dumps(settings).encode(),
            })
            models = root / "storage/models"
            models.mkdir(parents=True)
            (models / model.name).symlink_to(model)
            # A download leaves this integrity marker, so the app trusts the
            # file as it would an installed model instead of re-hashing it.
            (models / (model.name + ".sha256")).write_text(model_sha)
            env = dict(os.environ, CFFIXED_USER_HOME=str(root / "home"),
                       LOKALBOT_STORAGE_ROOT=str(root / "storage"),
                       LOKALBOT_DEFAULTS_SUITE=suite)
            # Go through cfprefsd instead of writing behind its cache. This
            # unique disposable domain cannot overwrite the installed settings.
            subprocess.run(["defaults", "import", suite, "-"], input=fixture,
                           env=env, check=True, capture_output=True)
            stdout_path = args.output / f"run-{index + 1}.stdout.log"
            stderr_path = args.output / f"run-{index + 1}.stderr.log"
            try:
                with stdout_path.open("w") as stdout, stderr_path.open("w") as stderr:
                    result = subprocess.run([str(app), "--cotyping-bench"], env=env,
                                            stdout=stdout, stderr=stderr, timeout=180)
            except subprocess.TimeoutExpired:
                raise SystemExit(f"Engine timed out; inspect {stderr_path}") from None
            finally:
                subprocess.run(["defaults", "delete", suite], env=env, capture_output=True)
            try:
                report = json.loads(stdout_path.read_text())
            except json.JSONDecodeError:
                raise SystemExit(f"No benchmark JSON (exit {result.returncode}); inspect {stderr_path}") from None
            (args.output / f"run-{index + 1}.json").write_text(json.dumps(report, indent=2) + "\n")
            # llama.cpp names the file it loads (temp paths may be printed
            # with /private); any other model means the isolated settings
            # were not applied.
            if f"/storage/models/{model.name}" not in stderr_path.read_text(errors="replace"):
                raise SystemExit(f"{model.name} was not loaded; isolated settings were not applied")
            # Exit 1 only means a scenario missed its contract: that is a
            # result to record, not a reason to stop the remaining runs.
            print(json.dumps({"run": index + 1, "exitCode": result.returncode,
                              **{key: value for key, value in report.items() if key != "scenarios"}}), flush=True)


if __name__ == "__main__":
    main()
