# zed-gherkin

[Zed](https://zed.dev) extension for Gherkin / Cucumber `.feature` files.

- Syntax highlighting via [tree-sitter-gherkin](https://github.com/berendkleinhaneveld/tree-sitter-gherkin)
- Go-to-definition and find-references for step calls, via a bundled language server (`gherkin-lsp`)
- Optional formatter (`gherkin-fmt`) for table alignment, consistent indentation, and blank-line normalization

## Install

To install from source:

```sh
git clone https://github.com/berendkleinhaneveld/zed-gherkin
```

In Zed: open the command palette and run `zed: install dev extension`, then pick the cloned directory.

## Language server (go-to-definition, find-references)

`gherkin-lsp` is a standalone Rust binary that indexes step definitions in your workspace and serves go-to-definition and find-all-references for step calls inside `.feature` files. It is launched automatically by the Zed extension once installed on your `PATH`:

```sh
cd crates/gherkin-lsp
cargo install --path .
```

Supported step-definition languages:

- **Python** — [`behave`](https://behave.readthedocs.io/) and [`pytest-bdd`](https://pytest-bdd.readthedocs.io/) (including `parsers.parse(...)` / `parsers.cfparse(...)` wrappers)
- **JavaScript / TypeScript** — [`@cucumber/cucumber`](https://github.com/cucumber/cucumber-js) (`Given`, `When`, `Then`, `And`, `But`, `defineStep`) with single-quote, double-quote, and backtick string literals

Cmd+click (or right-click → Go to Definition) on a step in a feature file jumps to the matching step definition. Right-click → Find All References from either side lists every call/definition that resolves to the same pattern.

### Supported Cucumber expressions

`{string}`, `{int}`, `{float}`, `{word}`, `{}` (anonymous), optional groups `(s)`, and slash alternation `apple/pear`. Unknown parameter types fall through to a permissive wildcard. Regex-literal step definitions (`Given(/^.../, ...)`) are not indexed in v1.

### Known limitations

- ASCII-only position reporting — multi-byte characters in step text may have off-by-N column offsets
- Single-line string literals only — step definitions split across lines are not indexed
- Custom parameter types are treated as wildcards
- The indexer walks the workspace on startup; large monorepos may take a moment on first open

## Formatter

`gherkin-fmt` is a standalone Rust CLI that reads a `.feature` file on stdin and writes the formatted version to stdout.

```sh
cd crates/gherkin-fmt
cargo install --path .
```

Wire it up in your Zed `settings.json`:

```json
"languages": {
  "Gherkin": {
    "format_on_save": "on",
    "formatter": {
      "external": { "command": "gherkin-fmt", "arguments": ["--tags-per-line"] }
    }
  }
}
```

Pass `--tags-per-line` if you prefer one tag per line over space-separated tags.

## What it does

- Indents `Feature:` / `Rule:` / `Scenario:` / steps / tables with a consistent 2-space scheme
- Aligns table columns; numeric columns are right-aligned
- Collapses runs of blank lines
- Leaves doc-string (`"""` / ``` ``` ```) contents untouched
- Normalizes tag groups onto a single line (or one per line with `--tags-per-line`)

## Repo layout

- `extension.toml`, `languages/gherkin/`, `src/lib.rs` — Zed extension wrapper (Rust, compiled to `wasm32-wasip1`)
- `crates/gherkin-fmt/` — formatter CLI
- `crates/gherkin-lsp/` — language server binary
- Grammar lives in the sibling repo [tree-sitter-gherkin](https://github.com/berendkleinhaneveld/tree-sitter-gherkin), pinned by rev in `extension.toml`

See `AGENTS.md` for development notes.

## License

MIT
