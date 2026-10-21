# CUDA hardware validation — 2026-10-21

The validation used synthetic prompts and the repository's fictional demo library.
The L4 run tested `f4425e736abf2f73905badebfa465aee71a6c010`; the RTX 3080 and
RTX 5060 Ti runs tested `3c732843dff1b71a942ee0149db86a286f791ae4`. These revisions
have identical runtime and test code; only documentation changed between them.

## Environment

Three separate Vast.ai hosts:

| GPU | Reported memory | Runtime and autocomplete result |
| --- | --- | --- |
| NVIDIA L4 | 23,034 MiB | Passed |
| NVIDIA GeForce RTX 3080 | 10,240 MiB | Passed |
| NVIDIA GeForce RTX 5060 Ti | 16,311 MiB | Passed |

Common software environment:

- Ubuntu 24.04 x86_64 container, glibc 2.39.
- NVIDIA driver 615.71.09; `cuDriverGetVersion` returned `13040`.
- Rust 1.99.1; llama.cpp b11669 (`620d2225c`), pinned CUDA 13.4 archives.
- Initially no loadable `libcudart.so.13`, `libcublas.so.13` or `libcublasLt.so.13`.

All three temporary instances were destroyed after collecting the results.

## Runtime tests

The ignored integration test passed twice on each card with
`LOKALBOT_E2E_BACKEND=cuda`:

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
On the L4, after the startup fix below, the bundled-library inference run reused
its previously downloaded files; the system-library run installed into a fresh
runtime directory. Both RTX cards started with fresh runtime directories for
both installation paths. The system-library runs set
`CUDA_HOME=/usr/local/cuda-13.4` and reused the already downloaded models.
All six runs used Qwen3.5 0.8B Q4_K_M and EmbeddingGemma 2 Q8_0 and checked:

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
marker confirmed CUDA model offload on every card. Word-by-word acceptance and
suggestion continuation also ran.

| GPU | Suggestions | Warm-up | Last key → suggestion p50 | p95 / max |
| --- | --- | --- | --- | --- |
| L4 | 19 / 20 | 2.3 s | 214 ms | 247 ms |
| RTX 3080 | 19 / 20 | 2.8 s | 274 ms | 360 ms |
| RTX 5060 Ti | 19 / 20 | 2.1 s | 183 ms | 205 ms |

Latency includes the 160 ms debounce and covers successful suggestions only.
One scenario on each card produced no suggestion within the evaluator's timeout.
These are observations from one debug-engine run per card on separate hosts,
not a controlled GPU performance comparison, quality score or production
performance guarantee. The evaluator simulates fields; it does not test
insertion into a real desktop application.

## Remaining coverage

- Native Windows NVIDIA execution remains untested; Windows compilation,
  workspace tests, packaging and hosted desktop checks run separately in CI.
- No CUDA/Vulkan speedup is claimed. The L4 container's Vulkan ICD failed
  to initialize. The RTX follow-up runs tested CUDA only.
- No performance claim is made for other GPUs, models, larger contexts or
  simultaneous workloads. Automatic remains Vulkan/CPU; CUDA is opt-in.
