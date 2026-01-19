# konf-lsp

Language Server Protocol implementation for konf-provider YAML configuration files.

## Features

- **Autocompletion**
  - Template references: `${file.key.path}` with type hints
  - Import paths in `<!>:` metadata section
  - Keys from imported config files

- **Diagnostics**
  - Invalid import references
  - Invalid template references (unimported files, unknown keys)
  - Circular import detection
  - Type warnings (complex types in string interpolation)

- **Navigation**
  - Go to definition for imports and template references
  - Hover information showing referenced values

## Building

### Avec just (recommandé)

```bash
cd konf-lsp

# Build tout et package l'extension
just package

# Build et installer directement dans VSCode
just install

# Autres commandes disponibles
just --list
```

### Manuellement

```bash
# From the konf-lsp directory
cargo +nightly build --release

# The binary will be at target/release/konf-lsp
```

## Usage

### Standalone

```bash
# The LSP communicates via stdio
./target/release/konf-lsp
```

### With VSCode

1. Build the LSP binary
2. Install the VSCode extension:

```bash
cd vscode-extension
bun install
bun run compile
bun run package  # Creates a .vsix file
```

3. Install the `.vsix` file in VSCode:
   - Open VSCode
   - Press `Ctrl+Shift+P` / `Cmd+Shift+P`
   - Type "Install from VSIX"
   - Select the generated `.vsix` file

4. Configure the extension (optional):
   - Set `konf.serverPath` to point to the `konf-lsp` binary
   - Or ensure `konf-lsp` is in your `PATH`

## Configuration

The VSCode extension supports these settings:

| Setting | Description | Default |
|---------|-------------|---------|
| `konf.serverPath` | Path to the konf-lsp binary | (auto-detect) |
| `konf.trace.server` | Trace communication with the server | `off` |

## Development

### Running the LSP in debug mode

```bash
RUST_LOG=konf_lsp=debug cargo +nightly run
```

### Testing with VSCode

1. Open the `vscode-extension` folder in VSCode
2. Press `F5` to launch a new Extension Development Host
3. Open a folder containing konf YAML files
4. Test autocompletion, diagnostics, and navigation

## Architecture

```
konf-lsp/
├── src/
│   ├── main.rs         # LSP server entry point
│   ├── parser.rs       # YAML parsing with konf-specific features
│   ├── workspace.rs    # Workspace indexing and caching
│   ├── completion.rs   # Autocompletion provider
│   └── diagnostics.rs  # Error/warning diagnostics
└── vscode-extension/
    ├── src/
    │   └── extension.ts  # VSCode extension entry point
    ├── syntaxes/         # TextMate grammar for syntax highlighting
    └── package.json      # Extension manifest
```
