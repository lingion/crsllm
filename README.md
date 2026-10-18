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
  <a href="https://github.com/lingion/crsllm/releases"><img src="https://img.shields.io/badge/crsllm-0.1.1-4facfe?style=flat-square" alt="crsllm version 0.1.1"></a>
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

## Features

### Provider Management & Model Discovery

<p align="center">
  <img src="docs/screenshots/console-providers.png" width="100%" alt="CRSLLM console — providers">
</p>

- **81 pre-configured providers** (OpenAI, Anthropic, Google, Azure, AWS Bedrock, etc.)
- **Auto-discovery**: One-click to fetch all available models from each provider
- **Health monitoring**: Real-time latency, success rate, and request counts
- **Enable/disable** providers instantly without restart

### Model Groups — Automatic Failover

<p align="center">
  <img src="docs/screenshots/console-groups.png" width="100%" alt="CRSLLM console — model groups">
</p>

- **Logical model abstraction**: Map `gpt-4o` to multiple providers with automatic failover
- **Priority-based routing**: Ordered provider list with automatic fallback on errors
- **Cross-provider fallback**: If OpenAI fails, automatically try Anthropic, then Google
- **Zero-config discovery**: Auto-generated model groups from provider capabilities

### Routed Models — Client-Facing Aliases

<p align="center">
  <img src="docs/screenshots/console-routed.png" width="100%" alt="CRSLLM console — routed models">
</p>

- **Stable API names**: Expose `my-gpt4` to clients, route to any backend model
- **Hot-swap backends**: Change underlying models without client code changes
- **A/B testing**: Route percentage of traffic to different model groups
- **Version management**: Seamlessly upgrade from `gpt-4-0613` to `gpt-4o`

### Fallback Chain — Global Resilience

<p align="center">
  <img src="docs/screenshots/console-fallback.png" width="100%" alt="CRSLLM console — fallback chain">
</p>

- **Cross-group failover**: If entire model group fails, try next in chain
- **Configurable order**: Drag-and-drop priority adjustment
- **Automatic recovery**: Failed groups are retried after cooldown
- **Circuit breaker**: Prevents cascading failures

### API Keys & Access Control

<p align="center">
  <img src="docs/screenshots/console-keys.png" width="100%" alt="CRSLLM console — API keys">
</p>

- **Multi-key support**: Create separate keys for different clients/apps
- **Key rotation**: Generate new keys without downtime
- **Usage tracking**: Monitor requests per API key
- **Instant revocation**: Disable compromised keys immediately

### Request Logs & Observability

<p align="center">
  <img src="docs/screenshots/console-logs.png" width="100%" alt="CRSLLM console — logs">
</p>

- **Real-time logs**: Live request stream with status, latency, and model used
- **Error tracking**: Detailed error messages and upstream provider responses
- **Performance metrics**: TTFT (Time To First Token), total latency, token counts
- **Filtering**: Search by model, provider, status, or time range

## What is CRSLLM?

CRSLLM is a **single-binary, self-hosted gateway** in front of all your LLM providers. Clients speak one of three dialects; the gateway translates, routes, and fails over:

- **`POST /v1/messages`** — Anthropic Messages API (Claude Code, Anthropic SDKs)
- **`POST /v1/chat/completions`** — OpenAI Chat Completions API (Codex, most SDKs/tools)
- **`POST /v1/responses`** — OpenAI Responses API (latest OpenAI SDK, agents, tool use)

Requests are resolved through **model groups** (an ordered provider list per logical model) and a **fallback chain** (cross-group failover order). Every upstream error is classified (`429` rate limit, `402` quota, `403` forbidden, `404` model not found, timeout, …) and triggers an **immediate retry on the next provider** — including mid-stream SSE failures (inactivity timeout + abrupt-EOF detection). An 81-provider registry ships in `config/providers.toml` (base URL, auth style, model-discovery path).

### Feature list

| Area | What you get |
|---|---|
| Interfaces | Anthropic `/v1/messages` + OpenAI `/v1/chat/completions` + OpenAI `/v1/responses`, transparent cross-translation |
| Routing | Model groups · routed models · global fallback chain · per-provider priority & enable/disable |
| Resilience | Error classification with instant failover, SSE streaming with inactivity + EOF guarding, per-stage timeouts (connect/TTFT/generation/total) |
| Discovery | `POST /api/refresh-models` probes every provider concurrently and caches results; `GET /v1/models` then serves OpenAI-shaped `data[]` instantly |
| Access control | Client API keys (via `x-api-key` header or `Authorization: Bearer`), create/rotate/revoke from the console |
| Console | Live web dashboard: providers, groups, routes, fallback, API keys, request logs, metrics sparklines |
| Observability | `/metrics`, request history, per-provider success/TTFT stats persisted in config |
| CLI | `--help`, `--init`, `--version`, `--port`, `--config`, `--discover-models`; auto-config on first run |

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
│   ├── responses.rs       # OpenAI Responses API types, translation, streaming events
│   ├── error.rs           # error classification (ported failover semantics)
│   ├── metrics.rs         # per-provider stats, success rate, TTFT percentiles
│   ├── model_resolver.rs  # logical name → provider-native model id mapping
│   ├── adapters/          # anthropic / openai / google upstream adapters
│   └── streaming/         # SSE passthrough, inactivity timeout, EOF detection
├── web/dashboard.html     # the console (embedded into the binary at build time)
├── config/
│   ├── crsllm.toml.example  # annotated reference config — copy to crsllm.toml
│   └── providers.toml       # 81-provider registry (base_url / auth_type / models_path)
├── scripts/
│   └── install.sh           # one-command install to ~/.cargo/bin
├── tests/integration.rs     # end-to-end HTTP tests incl. auth enforcement
├── Makefile                 # build, run, install, test, discover targets
└── docs/                    # mark, screenshots, provider catalog
```

## Requirements

- Rust 1.85+ (edition 2024) — [rustup](https://rustup.rs)
- macOS / Linux / Windows. No database, no external services — one binary + one TOML.

## Quick start

```bash
git clone https://github.com/lingion/crsllm && cd crsllm

# Option A: one-command install
make install          # builds release binary → ~/.cargo/bin/crsllm
crsllm --init         # create config from template, then add your keys
crsllm                # start the gateway on :3002

# Option B: run from source
make run              # auto-creates config if missing, builds & runs
```

Or manually:

```bash
cargo build --release
./target/release/crsllm --init    # create config from template
./target/release/crsllm           # listens on 0.0.0.0:3002
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

CLI reference:

```bash
crsllm --help                          # show all options
crsllm --version                       # show version
crsllm --init                          # create config/crsllm.toml from template
crsllm --port 8080                     # custom port (default 3002)
crsllm --config path/to/crsllm.toml    # custom config path
crsllm --discover-models               # probe every provider, save discoveries
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
| `POST /v1/responses` | client key* | OpenAI Responses API (agents, tool use, latest SDK) |
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
cargo test            # unit + integration (95 tests): routing, failover, auth, translation, responses API
cargo run             # dev server on :3002 with env-filter tracing
make help             # show all build/run/install targets
```

## License

Apache-2.0. Error-classification and failover semantics draw on ideas proven in FreeLLMAPI; the gateway, console, and routing are original Rust.

## Maintained By

- **Lingion**: gateway core, console, infrastructure, and repository quality
