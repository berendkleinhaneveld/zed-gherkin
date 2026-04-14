# zed-gherkin

[Zed](https://zed.dev) extension for Gherkin / Cucumber `.feature` files.

- Syntax highlighting via [tree-sitter-gherkin](https://github.com/berendkleinhaneveld/tree-sitter-gherkin)
- Optional formatter (`gherkin-fmt`) for table alignment, consistent indentation, and blank-line normalization

## Install

To install from source:

```sh
git clone https://github.com/berendkleinhaneveld/zed-gherkin
```

In Zed: open the command palette and run `zed: install dev extension`, then pick the cloned directory.

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

- `extension.toml`, `languages/gherkin/` — Zed extension wrapper
- `crates/gherkin-fmt/` — formatter CLI (published here, installable via `cargo install --path`)
- Grammar lives in the sibling repo [tree-sitter-gherkin](https://github.com/berendkleinhaneveld/tree-sitter-gherkin), pinned by rev in `extension.toml`

See `AGENTS.md` for development notes.

## License

MIT
