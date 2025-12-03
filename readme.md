# konf-provider

A configuration server that serves YAML configuration files with templating support. It can source configs from a local filesystem or a git repository, and output in multiple formats.

## Features

- **Multiple sources**: Serve configs from local filesystem or git repository
- **Templating**: Reference values from imported config files using `${path.to.value}` syntax
- **Multiple output formats**: yaml, json, env, properties, toml, docker_env
- **Hot reload**: Reload configs without restarting the server
- **Git integration**: Serve configs at specific commits with token-based access control
- **Observability**: Prometheus metrics and OpenTelemetry tracing support

## Requirements

- Rust nightly (edition 2024)

## Build

```bash
# Build debug
cargo +nightly build

# Build release
cargo +nightly build --release
```

## Usage

### Local Mode

Serve configuration files from a local directory:

```bash
cargo +nightly run --bin server -- local --folder /path/to/configs [--port 4000]
```

### Git Mode

Serve configuration files from a git repository:

```bash
cargo +nightly run --bin server -- git \
    --repo-url <url> \
    --branch <branch> \
    [--username <user> --password <pass>] \
    [--port 4000]
```

### Environment Variables

- `KONF_PORT`: Set the server port (alternative to `--port` flag)
- `OTEL_EXPORTER_OTLP_ENDPOINT`: OpenTelemetry collector endpoint (e.g., `http://localhost:4317`)
- `RUST_LOG`: Log level configuration (e.g., `konf_provider=debug,tower_http=debug`)

## Configuration Files

### Metadata Section

Config files support a `<!>` metadata section:

```yaml
<!>:
  import:
    - common/base.yaml
    - common/secrets.yaml
  auth:
    - my-secret-token  # git mode only

database:
  host: ${common.database.host}
  port: ${common.database.port}
```

- `import`: List of other config files to import (without file extension)
- `auth`: List of tokens that can access this config (git mode only)

### Nested Folder Structure

Configuration files can be organized in nested folders. Import paths use forward slashes:

```
configs/
  common/
    database.yaml
    redis.yaml
  services/
    api/
      config.yaml
    worker/
      config.yaml
```

Import nested configs using their relative path (without extension):

```yaml
# services/api/config.yaml
<!>:
  import:
    - common/database
    - common/redis

database:
  url: postgres://${common/database.user}@${common/database.host}:${common/database.port}
```

### Template Syntax

Use `${path.to.value}` to reference values from imported files:

```yaml
<!>:
  import:
    - base

app:
  name: my-app
  database_url: postgres://${base.db.user}:${base.db.password}@${base.db.host}:${base.db.port}
```

For nested imports, use the full relative path as the prefix:

```yaml
<!>:
  import:
    - common/database

connection: ${common/database.host}:${common/database.port}
```

### Complete Example

Given these configuration files:

**common/database.yaml**
```yaml
host: localhost
port: 5432
name: myapp_db
user: app_user
password: secret123
```

**common/redis.yaml**
```yaml
host: localhost
port: 6379
db: 0
```

**services/api/config.yaml**
```yaml
<!>:
  import:
    - common/database
    - common/redis

service:
  name: api-service
  port: 8080

database:
  url: postgres://${common/database.user}:${common/database.password}@${common/database.host}:${common/database.port}/${common/database.name}

cache:
  url: redis://${common/redis.host}:${common/redis.port}/${common/redis.db}
```

Requesting `GET /data/yaml/services/api/config` returns:

```yaml
service:
  name: api-service
  port: 8080
database:
  url: postgres://app_user:secret123@localhost:5432/myapp_db
cache:
  url: redis://localhost:6379/0
```

Or as JSON (`GET /data/json/services/api/config`):

```json
{
  "service": {
    "name": "api-service",
    "port": 8080
  },
  "database": {
    "url": "postgres://app_user:secret123@localhost:5432/myapp_db"
  },
  "cache": {
    "url": "redis://localhost:6379/0"
  }
}
```

## API Endpoints

### Health Check

```
GET /live
```

### Prometheus Metrics

```
GET /metrics
```

Returns Prometheus-formatted metrics for monitoring.

### Reload Configs

```
POST /reload
```

### Get Config (Local Mode)

```
GET /data/:format/*path
```

Example: `GET /data/json/myapp/config`

### Get Config (Git Mode)

```
GET /data/:commit/:format/*path
```

Requires `token` header for authentication.

Example: `GET /data/abc123/yaml/myapp/config` with header `token: my-secret-token`

## Output Formats

| Format | Description |
|--------|-------------|
| `yaml` | YAML format |
| `json` | JSON format |
| `env` | Shell environment variables (`export KEY=value`) |
| `properties` | Java properties format (`key=value`) |
| `toml` | TOML format |
| `docker_env` | Docker env file format (`KEY=value`) |

## Observability

### Prometheus Metrics

The following metrics are exposed at `/metrics`:

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `http_requests_total` | Counter | `method`, `path`, `status` | Total HTTP requests |
| `http_request_duration_seconds` | Histogram | `method`, `path`, `status` | Request duration |
| `config_reloads_total` | Counter | `success` | Config reload operations |
| `config_renders_total` | Counter | `format`, `success` | Config render operations |
| `config_render_duration_seconds` | Histogram | `format`, `success` | Render duration |
| `git_cache_lookups_total` | Counter | `hit` | Git DAG cache lookups (git mode only) |

### OpenTelemetry Tracing

Set `OTEL_EXPORTER_OTLP_ENDPOINT` to enable trace export to an OpenTelemetry collector:

```bash
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 cargo +nightly run --bin server -- local --folder /path/to/configs
```

## License

See [LICENSE](LICENSE) for details.
