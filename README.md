# PolarExplorer

A desktop sailing polar editor for macOS (Intel and Apple Silicon), Windows
(x64 and ARM64) and Linux (x64). Combine ORC certificates, imported polars and
race tracks into a polar for one boat, then export it to Expedition or Adrena.
The interface and built-in reference are available in English, French and German.

- [User guide](docs/USER-GUIDE.md): importing, weather, editing and export.
- [Data sources and credits](docs/DATA-SOURCES.md).
- [Release instructions](docs/RELEASING.md): platform builds and optional signing.
- [Development conventions](CLAUDE.md) and [UI testing](docs/AGENT-UI-TESTING.md).
- [Specification](spec.md) and [milestone status](plan.md).

The ORC catalogue and map are bundled. Network access happens only when you
import an event from a supported tracker or request environmental data.
Projects retain original sources, reversible edits and the weather sampled at
track positions; there is no persistent download cache.

## Develop

Install Rust 1.97 or later, Node.js with npm, and the
[Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/).
From the repository root:

```sh
npm ci
CARGO_INCREMENTAL=0 npm run dev
```

`npm run build` produces a native bundle for the host platform. See the release
instructions before distributing it: builds without credentials are not
notarized on macOS and are unsigned on Windows.

The workspace declares `PolyForm-Noncommercial-1.0.0` for application code.
Third-party datasets retain their own terms; the bundled ORC catalogue's
[MIT notice](docs/licenses/orc-data-MIT.txt) is included separately.

## Native AI assistant integrations

Connect Codex, Claude Code or Claude Desktop from Settings → AI assistants. The native plugin opens this app when needed and follows the selected workflow permissions. Routine reads, edits, downloads and exports can run without repeated approval. See the [shared MCP architecture and maintenance contract](vendor/whirlwind-mcp/README.md) for installation, migration, permission boundaries, jobs and validation.
