# Konf Provider - VSCode Extension

Language support for [konf-provider](https://github.com/your-org/konf-provider) YAML configuration files.

## Features

### Autocompletion

- **Template references** (`${file.key.path}`) - suggests imported files and their keys with type hints
- **Import paths** in `<!>:` metadata section - suggests available config files
- **Function chains** (`${value | trim | upper}`) - suggests available transform functions

### Diagnostics

- Invalid import references (file not found)
- Invalid template references (unimported files, unknown keys)
- Circular import detection
- Type warnings (complex types in string interpolation)

### Navigation

- **Go to Definition** - `Cmd+Click` / `F12` on imports or template references
- **Hover** - shows the resolved value from referenced configs

## Activation

The extension activates when a `.konf` marker file is present in the workspace root.

To enable the extension for your project:

```bash
touch .konf
```

## Configuration

| Setting | Description | Default |
|---------|-------------|---------|
| `konf.serverPath` | Path to the `konf-lsp` binary | Auto-detect |
| `konf.trace.server` | Trace LSP communication (`off`, `messages`, `verbose`) | `off` |

## Example Config

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

# With function chains
api_key: ${secrets.key | trim | upper}
```

## Requirements

- The `konf-lsp` language server binary must be available (bundled or in PATH)
- A `.konf` file in the workspace root to activate the extension

## Building from Source

```bash
cd konf-lsp

# Build and install
just install

# Or manually
cargo +nightly build --release
cd vscode-extension
bun install
bun run compile
bun run package
```

Then install the generated `.vsix` file via `Extensions: Install from VSIX...` in VSCode.
