# Uzu runtime plugin for MeshLLM

Run [Uzu](https://github.com/trymirai/uzu)'s Apple Silicon inference engine through
[MeshLLM](https://github.com/Mesh-LLM/mesh-llm). Mesh owns model discovery and
routing; this external plugin owns a local Uzu server and its lifecycle.

**Experimental.** Tested on an M5 Max with MeshLLM 0.78.1. Requires MeshLLM v0.77+
and Uzu-converted models; GGUF and Skippy split execution are not supported.
Prebuilt macOS releases and their release pipeline are not available yet.

## Performance

Best-effort Qwen3.8-27B comparison on one M5 Max (128 GB), October 28, 2026:

| Workload | Released Skippy 0.78.1 | Uzu | Speedup |
|---|---:|---:|---:|
| Compiler paragraph | ~21 tok/s | 59–66 tok/s | 2.8–3.1× |
| Rainbow explanation | 19–23 tok/s | 69–87 tok/s | 3.6–3.8× |
| Merge-sorted-lists code prefix | ~23 tok/s | 180–221 tok/s | 7.8–9.6× |

Ranges are warm medians across two passes (three measured requests per prompt
per pass, excluding a warmup). Same prompts, temperature 0, thinking disabled,
256-token output cap, serial requests, and no prompt-cache reuse. Throughput is
completion tokens divided by full HTTP request duration, measured against each
engine directly—not Mesh routing throughput. Warm first-text medians were
133–191 ms for Uzu and 289–382 ms for Skippy.

These are similar-footprint packages, **not identical quantizations**: Skippy
UD-Q4_K_M GGUF is 16.46 GB; Uzu mirai-m4 is 14.53 GB plus a 1.03 GB draft
(15.56 GB total). Uzu's speculative decoding is particularly effective on this
repetitive code prefix (~10.7 tokens per verification). The code is truncated,
not a completed coding task. A bounded Skippy N-gram suffix trial did not close
the gap; this is not an exhaustive tuning study or a latest-source comparison.
Thermal/desktop activity was uncontrolled, output text varies, and quality,
tool use, long-context performance, and speculative equivalence were not evaluated.
See [benchmark method and prompts](docs/PERFORMANCE.md).

## Build from source

The control executable retains the name `uzu-plugin`:

```sh
just verify
just build
```

Build the engine separately from Uzu commit
`0d60981a899c79fc1c29ab3ca1c5aeff937affff`. In that checkout, apply this repo's
[`uzu-server.patch`](uzu-server.patch), then build with Rust 1.99.0:

```sh
git apply /absolute/path/to/uzu-runtime-plugin/uzu-server.patch
RUSTC_BOOTSTRAP=1 cargo build --locked --release -p cli
```

Copy Uzu's `target/release/cli` beside `target/release/uzu-plugin`, naming it
`uzu-server`. Preserve [`UZU_LICENSE`](UZU_LICENSE) when distributing the engine.
The Mesh plugin SDK is pinned in `Cargo.toml` and `Cargo.lock`.

**Use the patched engine.** Stock Uzu CLI startup accesses application Keychain
settings and discovers other local providers. The patch removes that path for
credential-free unattended serving and selects its universal downloader.
`--engine /absolute/path/uzu-server` overrides the sibling executable.

## Configure MeshLLM

Add to your MeshLLM config (normally `~/.mesh-llm/config.toml`):

```toml
[runtime]
mode = "on_demand"

[[plugin]]
name = "uzu-plugin"
command = "/absolute/path/to/uzu-plugin"
args = ["--model", "alibaba:qwen3.8:27b:mirai:mirai-m:4"]
```

Start MeshLLM and query its OpenAI-compatible API:

```sh
mesh-llm serve
curl http://127.0.0.1:9337/v1/models
curl http://127.0.0.1:9337/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"alibaba:qwen3.8:27b:mirai:mirai-m:4","messages":[{"role":"user","content":"Explain how a compiler works."}],"max_tokens":256,"enable_thinking":false,"stream":true}'
```

The first launch downloads missing weights through Uzu; allow time for download
and loading. Its model cache is retained on shutdown. Use the exact model ID
returned by `/v1/models`. Uzu's catalog checked for this prototype had no Gemma
conversion; this plugin cannot load the ladder's Gemma GGUFs. The tested Qwen
conversion advertises no tool support; do not assume agent tool-call compatibility.

## Implementation and limitations

- Mesh IPC provides control/initialization and a managed HTTP endpoint. Tensors
  stay inside Uzu. The engine starts only after a compatible host handshake.
- A dedicated supervisor shuts down the owned backend on disconnect or plugin
  termination. Tests include plugin SIGKILL and unrelated-process safety.
- A loopback adapter decodes HTTP chunk framing before returning
  connection-delimited JSON/SSE to Mesh, working around framing failures observed
  with the tested host. It monitors cancellation; a live cancellation/fresh-request
  smoke passed. This is not exhaustive concurrency qualification.
- The adapter accepts Content-Length uploads up to 16 MiB, not chunked uploads,
  and has a 300-second timeout. It is not a general-purpose HTTP proxy.
- Single-node Apple Silicon only tested. No public/multi-node qualification,
  release-install test, plugin catalog publication, or automatic updater yet.

`just verify` runs formatting, warning-denying Clippy, and all seven plugin tests.
Tests use fake backends, never models or Keychain. `just clean` removes Cargo
outputs. The control plugin is Apache-2.0; Uzu is MIT-licensed.
