# Uzu inference plugin for Mesh

Experimental Apple Silicon engine plugin. Mesh owns routing; the plugin owns
an Uzu HTTP server. No inference tensors pass over plugin IPC. Single-node only;
this does not implement Skippy's split execution or package format.

Requires Mesh v0.77+ and macOS supported by Uzu (tested on M5 Max).

## Build

`just build` builds the control executable. Build the engine separately from
https://github.com/trymirai/uzu at commit
`0d60981a899c79fc1c29ab3ca1c5aeff937affff`, apply `uzu-server.patch`, then run
`RUSTC_BOOTSTRAP=1 cargo build --release -p cli` with Rust 1.99.0.
Place `target/release/cli` beside the plugin executable as `uzu-server`.
The patch avoids CLI Keychain settings and external local-provider discovery,
and selects Uzu's universal downloader for noninteractive execution.
It also buffers nonstreaming replies with an explicit Content-Length: the tested
Mesh host otherwise exposes Rocket HTTP chunk framing inside the JSON body.
This compatibility workaround sacrifices prompt disconnect cancellation for
nonstreaming requests; streaming retains the upstream cancellation path.
Uzu is MIT-licensed; preserve its LICENSE when distributing that binary.

## Configure Mesh

```toml
[runtime]
mode = "on_demand"
[[plugin]]
name = "uzu-plugin"
command = "/absolute/path/uzu-plugin"
args = ["--model", "alibaba:qwen3.8:27b:mirai:mirai-m:4"]
```

Run `mesh-llm serve`; query `/v1/models` and use its exact model ID for
`/v1/chat/completions`. Downloads happen through Uzu and remain in its model
cache on shutdown. Uzu-converted weights are required; GGUF is not supported.
There are no Gemma models in the upstream catalog checked for this prototype.

`--engine /absolute/path/uzu-server` overrides the sibling executable.
Do not point this at the unpatched stock CLI during unattended testing.

`just verify` runs formatting, Clippy and the entire package test suite.
Tests use a fake backend, never download weights or run models; lifecycle tests
cover disconnect, SIGKILL, incompatible protocol, and unrelated-process safety.
`just clean` removes disposable Cargo outputs.

Status: local prototype, not published to the plugin catalog or released.
