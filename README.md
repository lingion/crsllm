<p align="center">
  <img src="docs/crsllm-mark.svg" width="120" alt="crsllm project mark">
</p>

<h1 align="center">CRSLLM · Multi-Provider LLM Gateway</h1>

<p align="center">
  A standalone Rust gateway that unifies Anthropic & OpenAI APIs behind one endpoint,<br>
  with automatic provider failover and a live web console.<br>
  独立 Rust 多供应商 LLM 网关：统一 Anthropic / OpenAI 接口 · 自动故障转移 · 实时 Web 控制台
</p>

<p align="center">
  <a href="https://github.com/lingion/crsllm/releases"><img src="https://img.shields.io/badge/crsllm-0.1.0-4facfe?style=flat-square" alt="crsllm version 0.1.0"></a>
  <img src="https://img.shields.io/github/stars/lingion/crsllm?style=flat-square&logo=github" alt="Stars">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-4c9aff?style=flat-square" alt="Platforms">
  <img src="https://img.shields.io/badge/lang-Rust-dea584?style=flat-square&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/UI-Console%20%7C%20CLI%20%7C%20REST-43e97b?style=flat-square" alt="Interfaces">
  <img src="https://img.shields.io/badge/license-Apache--2.0-blue?style=flat-square" alt="License">
</p>

<p align="center">
  <a href="README.zh-CN.md">中文</a> · <a href="https://github.com/lingion/crsllm/releases">Releases</a> · <a href="#connecting-codex--claude-code">Codex / Claude Code setup</a>
</p>

---

> **Keywords (SEO):** LLM gateway, OpenAI-compatible proxy, Anthropic API gateway, provider failover, model routing, Rust LLM proxy, LiteLLM alternative, self-hosted AI gateway, Claude Code proxy, Codex proxy, API key management, SSE streaming gateway, 多模型网关, 供应商故障转移, 统一接口, 自建 LLM 网关

---

## Preview

<p align="center">
  <img src="docs/screenshots/console-providers.png" width="48%" alt="CRSLLM console — providers">
  <img src="docs/screenshots/console-keys.png" width="48%" alt="CRSLLM console — API keys">
</p>

## What is CRSLLM?

CRSLLM is a **single-binary, self-hosted gateway** in front of all your LLM providers. Clients speak one of two dialects; the gateway translates, routes, and fails over:

- **`POST /v1/messages`** — Anthropic Messages API (Claude Code, Anthropic SDKs)
- **`POST /v1/chat/completions`** — OpenAI Chat Completions API (Codex, most SDKs/tools)

Requests are resolved through **model groups** (an ordered provider list per logical model) and a **fallback chain** (cross-group failover order). Every upstream error is classified (`429` rate limit, `402` quota, `403` forbidden, `404` model not found, timeout, …) and triggers an **immediate retry on the next provider** — including mid-stream SSE failures (inactivity timeout + abrupt-EOF detection). An 81-provider registry ships in `config/providers.toml` (base URL, auth style, model-discovery path).

### Feature list

| Area | What you get |
|---|---|
| Interfaces | Anthropic `/v1/messages` + OpenAI `/v1/chat/completions`, transparent cross-translation |
| Routing | Model groups · routed models · global fallback chain · per-provider priority & enable/disable |
| Resilience | Error classification with instant failover, SSE streaming with inactivity + EOF guarding, per-stage timeouts (connect/TTFT/generation/total) |
| Discovery | `POST /api/refresh-models` probes every provider concurrently and caches results; `GET /v1/models` then serves OpenAI-shaped `data[]` instantly |
| Access control | Client API keys (via `x-api-key` header or `Authorization: Bearer`), create/rotate/revoke from the console |
| Console | Live web dashboard: providers, groups, routes, fallback, API keys, request logs, metrics sparklines |
| Observability | `/metrics`, request history, per-provider success/TTFT stats persisted in config |

## Why "CRSLLM"?

**C**ross-**R**outed **S**treaming **L**LM — a standalone Rust re-imagining that combines LiteLLM's routing feature set with FreeLLMAPI's battle-tested error-classification and timeout logic, plus a live control console neither one has.

## Repository layout

```
crsllm/
├── src/
│   ├── main.rs            # startup, CLI flags, router wiring, auth middleware
│   ├── handler.rs         # HTTP handlers + gateway_auth (client keys, loopback control plane)
│   ├── router.rs          # model-group resolution, fallback chain, failover loop
│   ├── config.rs          # TOML config, provider registry, key masking, save/reload
│   ├── translator.rs      # Anthropic ⇄ OpenAI request/response translation
│   ├── error.rs           # error classification (ported failover semantics)
│   ├── metrics.rs         # per-provider stats, success rate, TTFT percentiles
│   ├── model_resolver.rs  # logical name → provider-native model id mapping
│   ├── adapters/          # anthropic / openai / google upstream adapters
│   └── streaming/         # SSE passthrough, inactivity timeout, EOF detection
├── web/dashboard.html     # the console (embedded into the binary at build time)
├── config/
│   ├── crsllm.toml.example  # annotated reference config — copy to crsllm.toml
│   └── providers.toml       # 81-provider registry (base_url / auth_type / models_path)
├── tests/integration.rs     # end-to-end HTTP tests incl. auth enforcement
└── docs/                    # mark, screenshots, provider catalog
```

## Requirements

- Rust 1.85+ (edition 2024) — [rustup](https://rustup.rs)
- macOS / Linux / Windows. No database, no external services — one binary + one TOML.

## Quick start

```bash
git clone https://github.com/lingion/crsllm && cd crsllm
cargo build --release

cp config/crsllm.toml.example config/crsllm.toml   # then add your provider keys
./target/release/crsllm                            # listens on 0.0.0.0:3002
```

Open the console at **http://127.0.0.1:3002/dashboard** and you're operating:

| Tab | Do here |
|---|---|
| Providers | Add/edit/enable providers from the 81-registry, refresh their model lists, latency-test |
| Model Groups | Map a logical model name to an ordered list of providers (intra-group failover) |
| Routed Models | Alias a client-facing name to a chain of groups |
| Fallback Chain | Cross-group failover order when a whole group is down |
| **API Keys** | **Create the client keys for Codex / Claude Code / anything else** |
| Logs | Live request log: status, upstream used, latency, TTFT, token counts |

CLI extras:

```bash
crsllm --port 8080                                   # custom port (default 3002)
crsllm --discover-models [--config path/to/crsllm.toml]   # probe every provider, save discoveries
```

## Connecting Codex & Claude Code

**Where do I create the API key?** → Console → **API Keys** tab → **+ New Key**. The full key is shown **once** (copy it immediately; the list keeps only a masked `crsllm-1234***abcd` form). Deleting accepts either the full or masked form.

Auth rules (gateway middleware):

- If `[gateway] api_keys` is **empty/missing**, `/v1/*` is open — a fresh install can't lock you out. The first key you create switches it to key-required.
- Clients authenticate with `x-api-key: crsllm-…` **or** `Authorization: Bearer crsllm-…` (both accepted on `/v1/*`).
- `/health` and `/metrics` stay open; the control plane (`/api/*`, `/dashboard`) is **loopback-only** even though the server binds `0.0.0.0`.

### Claude Code (Anthropic-compatible)

```bash
export ANTHROPIC_BASE_URL="http://127.0.0.1:3002"
export ANTHROPIC_API_KEY="crsllm-x…-…"   # the key from the console
claude
```

### OpenAI Codex CLI

`~/.codex/config.toml`:

```toml
[model_providers.crsllm]
name = "CRSLLM gateway"
base_url = "http://127.0.0.1:3002/v1"
env_key = "CRSLLM_API_KEY"

[defaults]
model_provider = "crsllm"
```

```bash
export CRSLLM_API_KEY="crsllm-x…-…"
codex
```

Any OpenAI/Anthropic-compatible client works the same way: point its `base_url` at the gateway and use a console-created key.

## Configuration

Everything lives in one annotated file — see [`config/crsllm.toml.example`](config/crsllm.toml.example):

```toml
[gateway]
api_keys = []                 # client keys for /v1/* (empty = open)

[providers.openai]
api_key  = "sk-..."
base_url = "https://api.openai.com"
priority = 50                 # optional; performance scoring also adjusts routing
enabled  = true

[models."gpt-4o"]             # model group: intra-group ordered failover
providers = [
  { provider = "openai", model_id = "gpt-4o" },
  { provider = "openai", model_id = "gpt-4o-2024-08-06" },
]

[fallback_chain]              # cross-group failover when a group is fully down
order = ["gpt-4o", "claude-sonnet-4"]

[timeouts]
sse_inactivity_secs = 90
deadline_secs       = 120
```

Changes made in the console are written back to `config/crsllm.toml` and hot-reloaded — no restart needed.

## API surface

| Method & path | Auth | Purpose |
|---|---|---|
| `POST /v1/messages` | client key* | Anthropic Messages (Claude dialect) |
| `POST /v1/chat/completions` | client key* | OpenAI Chat Completions (translated internally) |
| `GET /v1/models` | client key* | List cached models (OpenAI shape, no live probing) |
| `POST /api/refresh-models` | loopback | Re-probe all providers concurrently, update cache & auto-groups |
| `GET /health` | open | Liveness probe |
| `GET /metrics` | open | Aggregated per-provider stats |
| `GET /dashboard` | loopback | Embedded web console |
| `GET/POST/DELETE /api/api-keys[/:key]` | loopback | Manage client keys (list is masked-only) |
| `GET/POST/PUT/DELETE /api/providers…` | loopback | Provider CRUD, enable/disable, refresh, test |
| `GET/POST/PUT/DELETE /api/model-groups…` | loopback | Model-group CRUD |
| `GET/PUT /api/fallback-chain` | loopback | Global fallback order |
| `GET/POST/PUT/DELETE /api/routed-models…` | loopback | Routed-model alias CRUD |
| `GET /api/logs` · `GET /api/metrics/history` | loopback | Console data |

\* required only when `[gateway] api_keys` is non-empty.

## Development

```bash
cargo test            # unit + integration (49 tests): routing, failover, auth, translation
cargo run             # dev server on :3002 with env-filter tracing
```

## License

Apache-2.0. Error-classification and failover semantics draw on ideas proven in FreeLLMAPI; the gateway, console, and routing are original Rust.

## Maintained By

- **Lingion**: gateway core, console, infrastructure, and repository quality
