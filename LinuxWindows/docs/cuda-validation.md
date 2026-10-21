# CUDA hardware validation — 2026-10-21

Runtime and test code revision: `f4425e736abf2f73905badebfa465aee71a6c010`.
The validation used synthetic prompts and the repository's fictional demo library.

## Environment

- Vast.ai NVIDIA L4, 23,034 MiB reported physical memory.
- Ubuntu 24.04 x86_64 container, glibc 2.39.
- NVIDIA driver 615.71.09; `cuDriverGetVersion` returned `13040`.
- Rust 1.99.1; llama.cpp b11669 (`620d2225c`), pinned CUDA 13.4 archives.
- Initially no loadable `libcudart.so.13`, `libcublas.so.13` or `libcublasLt.so.13`.

The temporary instance was destroyed after collecting the results.

## Runtime tests

The ignored integration test was run twice with `LOKALBOT_E2E_BACKEND=cuda`:

```sh
LOKALBOT_E2E_ROOT=/workspace/cuda-validation/bundled \
LOKALBOT_E2E_BACKEND=cuda \
cargo test --locked -p lokalbot-ai --test e2e_runtime \
  installs_runtime_downloads_models_and_answers -- --ignored --nocapture --test-threads=1
```

| Setup | Result |
| --- | --- |
| Required CUDA libraries absent | Installer downloaded and verified the server plus companion archive, about 594 MB total. Full inference test passed with `cudaLibraries` recording the companion digest. |
| Matching libraries available in `/usr/local/cuda-13.4/lib64` | A fresh runtime installation downloaded only the 154 MB server archive. Full inference test passed with `cudaLibraries: "system"` and none of the three companion libraries in the new runtime directory. |

The system-library test placed the exact matching libraries from the first
installation in a standard toolkit location. It establishes reuse of compatible
libraries, not compatibility with every CUDA toolkit version or distribution.
After the startup fix below, the bundled-library inference run reused its
previously downloaded files; the system-library run installed into a fresh
runtime directory. Both runs used Qwen3.5 0.8B Q4_K_M and EmbeddingGemma 2 Q8_0
and checked:

- Nonempty text generation, token counting, constrained JSON and streaming.
- 768-dimensional embeddings and higher similarity for the related document.
- Both Main and Embedder running with CUDA and `gpuOffloaded: true` markers.
- Clean shutdown of the managed servers.

### Startup regression caught by hardware execution

Revision `392f1a85f67eda42ba8d7bceead1b5c9a7769d8e` failed the integration test:
the CUDA device was detected and the server became healthy, but the runtime
could not confirm model-layer offload. The pinned server's default verbosity
level 3 omits the offload summary. At level 4, the same GPU and model reported:

```text
0.02.021.855 I load_tensors: offloaded 25/25 layers to GPU
```

CUDA launches now explicitly use `--log-verbosity 4 --log-colors off --no-log-jsonl`
so the existing stderr parser receives that evidence. The test failed before
this change and passed afterward; it also requires both exercised roles to be
running and positively marked as offloaded.

## Autocomplete smoke run

```sh
cargo run --locked -p lokalbot-engine --example autocomplete_eval -- \
  /workspace/cuda-validation/autocomplete-cuda --backend cuda
```

The headless evaluator used LFM2.5 1.2B Instruct Q4_K_M, a 4,096-token context,
160 ms debounce and simulated keystrokes 140 ms apart. The autocomplete server's
marker confirmed CUDA model offload. Warm-up took 2.3 seconds. Of 20 synthetic
scenarios, 19 produced a suggestion; one produced none within the evaluator's
timeout. Among the successful cases, last-keystroke-to-suggestion latency was
214 ms p50 and 247 ms p95/max, including debounce. Word-by-word acceptance and
suggestion continuation also ran.

These are observations from one run with a debug engine build, not a quality
score or a production performance guarantee. The evaluator simulates fields;
it does not test insertion into a real desktop application.

## Remaining coverage

- Native Windows NVIDIA execution remains untested; Windows compilation,
  workspace tests, packaging and hosted desktop checks run separately in CI.
- No CUDA/Vulkan speedup is claimed. The rented container's Vulkan ICD failed
  to initialize, so it could not provide a same-machine comparison.
- No performance claim is made for other GPUs, models, larger contexts or
  simultaneous workloads. Automatic remains Vulkan/CPU; CUDA is opt-in.
