# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run Commands

Requires Rust 1.85+ (edition 2024).

```bash
# Build the project
cargo build

# Build release
cargo build --release

# Run linter
cargo clippy

# Run the server in local mode (serves config from filesystem)
cargo run --bin server -- local --folder /path/to/configs [--port 4000]

# Run the server in git mode (serves config from git repository)
cargo run --bin server -- git --repo-url <url> --branch <branch> [--username <user> --password <pass>] [--port 4000]

# Port can also be set via environment variable
KONF_PORT=8080 cargo run --bin server -- local --folder /path/to/configs

# Build the cache helper binary
cargo build --bin cache

# Build the unified dev CLI (render + LSP)
cargo build --bin konf

# Render a config file locally (for testing before pushing)
cargo run --bin konf -- render -f /path/to/configs -n myconfig
cargo run --bin konf -- render -f /path/to/configs -n myconfig -o json
cargo run --bin konf -- render -f /path/to/configs -n myconfig -o env

# Available output formats: yaml (default), json, env, properties, toml, docker_env

# Start the LSP server (for IDE integration)
cargo run --bin konf -- lsp
```

## Architecture

konf-provider is a configuration server that serves YAML configuration files with templating support. It can source configs from a local filesystem or a git repository, and output in multiple formats (yaml, json, env, properties, toml, docker_env).

### Core Data Flow

1. **FileProvider** (`src/fs/`) - Abstracts file loading from local filesystem (`local.rs`) or git repository (`git.rs`)
2. **Loader** (`src/loader.rs`, `src/loaders/`) - Parses files into internal `Value` type (currently only YAML supported)
3. **Dag** (`src/render.rs`) - Stores loaded configs and handles rendering with dependency resolution
4. **ValueWriter** (`src/writer/`) - Serializes `Value` to output formats

### Key Types

- `Value` (`src/lib.rs`) - Internal representation of config data (String, Sequence, Mapping, Number, Boolean, Null)
- `Dag<P: FileProvider>` - Holds config files and renders them with template resolution. Uses `ArcSwap` for atomic reloads
- `MultiLoader` / `MultiWriter` - Dispatch to appropriate loader/writer based on file extension

### Templating System

Config files support a `<!>` metadata section with:
- `import`: list of other config files to import
- `auth`: list of tokens that can access this config (git mode only)

Template syntax uses `${path.to.value}` to reference values from imported files. See `src/render_helper.rs` for resolution logic.

### HTTP Endpoints

- `/live` - Health check
- `/metrics` - Prometheus metrics endpoint
- `/reload` - Reload configs from source
- `/data/:format/*path` (local mode) - Get rendered config
- `/data/:commit/:format/*path` (git mode) - Get rendered config at specific commit (requires `token` header for auth)

### Observability

- **Metrics** (`src/metrics.rs`) - Prometheus metrics using `metrics` crate (http requests, config renders, reloads, git cache)
- **Telemetry** (`src/telemetry.rs`) - OpenTelemetry tracing setup with optional OTLP export (set `OTEL_EXPORTER_OTLP_ENDPOINT`)

### Server Modes

- **Local mode** (`LocalAppState`) - Simple filesystem-based serving with hot reload
- **Git mode** (`GitAppState`) - Serves configs from git repo, caches DAGs per commit in `DashMap`, uses `Authorizer` for token-based access control

### LSP (Language Server Protocol)

The LSP implementation lives in `src/lsp/` (integrated into the main library) and provides IDE support (autocompletion, diagnostics, go-to-definition) for konf config files. It's bundled with the `konf` CLI binary for simplified distribution.

**IMPORTANT: The LSP MUST reuse core library code whenever possible.** Never duplicate logic that exists in the core library. This is critical because:

1. **Compatibility**: The LSP must behave exactly like the server. If parsing logic differs, the LSP may show incorrect completions or miss errors that the server would catch.
2. **Single source of truth**: Any bug fix or feature in core should automatically benefit the LSP.
3. **Accuracy**: The LSP must accurately represent what the server will do with a config file.

Currently shared from core (`konf_provider`):
- `imports::ImportInfo` - Import declaration structure
- `imports::parse_imports_from_yaml()` - Parse imports from serde_yaml::Value
- `imports::METADATA_KEY` - The `<!>` metadata key constant
- `render_helper::template_re()` - Regex for matching template references `${...}`
- `render_helper::TemplateRef` - Template reference with position information (line, column)
- `render_helper::find_template_refs()` - Find all template references in text with positions

When adding new parsing or validation logic:
1. First implement it in the core library
2. Export it for use by the LSP
3. Import and use it in the LSP - do NOT copy/paste or reimplement
