# Qwen3.8-27B measurement

Measured October 28, 2026 on Apple M5 Max, 128 GB. This is a practical comparison
of available packages, not quantization/quality parity or a general engine ranking.

## Inputs

Each engine receives an OpenAI chat request containing one user message:

1. `Write one paragraph explaining how a compiler works.`
2. `Write a Python function that merges two sorted lists. Explain its complexity.`
3. `Explain how rainbows form, then describe primary and secondary rainbows in detail.`

Parameters: `temperature: 0`, `max_tokens: 256`, `enable_thinking: false`,
`chat_template_kwargs: {"enable_thinking": false}`, `stream: true`,
`stream_options: {"include_usage": true}`. Both reported 21, 26, and 29 prompt
tokens respectively. No reasoning text was emitted. Each group runs one warmup
followed by three measured requests; repeat with fresh server processes.
Parse every SSE JSON event and require `[DONE]`. Use the final reported
`usage.completion_tokens` divided by wall time from request start to stream end.
Measure first text separately; do not call end-to-end tok/s pure decode speed.

## Engines

- Uzu source: `0d60981a899c79fc1c29ab3ca1c5aeff937affff`, with this repo's
  credential-free startup patch. Default speculation, prefix caching disabled
  (`server --no-prefix-cache`). Model:
  `alibaba:qwen3.8:27b:mirai:mirai-m:4`, checkpoint
  `3d1fe988a0ee35770a5051f3c89912f383adc46e`.
- Skippy via released MeshLLM 0.78.1, release-attested commit
  `70238ad1048498b24c524453800599b591131d35`. Metal runtime,
  `serve --local-model-only`, context 8192, parallel 1, disk KV off,
  n_batch/n_ubatch 512, 66/66 layers offloaded. Model: cached
  `unsloth/Qwen3.8-27B-GGUF:UD-Q4_K_M`. Repeat explicitly sets
  `[defaults.speculative] strategy = "disabled"`. No prompt cache hits reported.

Weight bytes: GGUF 16,464,440,224; Uzu target 14,532,457,872; Uzu draft
1,032,178,224. Uzu target+draft is 5.5% smaller overall. Similar size is a rough
proxy only; formats, quantization error and effective memory traffic differ.

## Warm medians

| Workload | Skippy first | Uzu first | Skippy repeat | Uzu repeat |
|---|---:|---:|---:|---:|
| Compiler paragraph, tok/s | 21.4 | 65.7 | 21.2 | 59.4 |
| Code prefix, tok/s | 22.9 | 179.5 | 22.9 | 220.8 |
| Rainbow explanation, tok/s | 19.5 | 69.3 | 23.2 | 87.2 |
| Compiler first text, ms | 289 | 133 | 382 | 152 |
| Code first text, ms | 294 | 182 | 322 | 159 |
| Rainbow first text, ms | 327 | 191 | 325 | 163 |

A Skippy `ngram-suffix` trial with min=3/max=12 produced 16.6, 18.4, and
20.5 tok/s respectively; no compatible learned draft or MTP pair was tested.
Uzu reported ~2.8, ~3.9 and ~10.7 completion tokens per verification on compiler,
rainbow and code respectively. This is a useful clue to workload sensitivity,
not a 27B speculation ablation: Uzu without speculation was not measured here.

## Limits

Order was Skippy → Uzu → Skippy N-gram → Skippy disabled → Uzu repeat, not
randomized or reversed. Only one test engine was live at a time. Thermal/power
state and desktop activity were uncontrolled; the variation is visible above.
Prose outputs differ and lengths vary. Code and rainbow requests hit the
256-token cap; the code is not a completed or correctness-tested task. No
long-context, quality, tool-call, energy, or latest-Skippy-source comparison.

This comparison uses direct local engine HTTP surfaces. A separate plugin smoke
proved routed Qwen discovery, JSON, SSE and cancellation through Mesh, but the
multipliers above must not be described as a routed-Mesh benchmark.
