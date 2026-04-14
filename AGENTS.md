# AGENTS.md

Zed extension for Gherkin / Cucumber `.feature` files. Three pieces:

- `tree-sitter-gherkin/` — grammar + corpus tests (its own git repo)
- `languages/gherkin/` + `extension.toml` — Zed extension wrapper
- `crates/gherkin-fmt/` — standalone Rust CLI used as Zed's external formatter

## Environment gotcha

The user's zsh profile defines broken `node()` / `npm()` functions. **Always invoke node as `/opt/homebrew/bin/node`**, and run JS CLIs directly:

```sh
/opt/homebrew/bin/node tree-sitter-gherkin/node_modules/tree-sitter-cli/cli.js <cmd>
```

`cargo`, `rustc`, `gherkin-fmt` (installed via `cargo install --path crates/gherkin-fmt`) are on PATH normally.

## Grammar development loop (TDD)

From `tree-sitter-gherkin/`:

```sh
/opt/homebrew/bin/node node_modules/tree-sitter-cli/cli.js generate
/opt/homebrew/bin/node node_modules/tree-sitter-cli/cli.js test
/opt/homebrew/bin/node node_modules/tree-sitter-cli/cli.js parse /tmp/sample.feature
/opt/homebrew/bin/node node_modules/tree-sitter-cli/cli.js highlight /tmp/sample.feature
```

Corpus tests live in `test/corpus/*.txt`. The format is:

```
=========
test name
=========
<input>
---
<expected s-expr>
```

**Always add a failing corpus test before touching `grammar.js`.** Run `generate` then `test` on every change.

`tree-sitter highlight` needs `~/Library/Application Support/tree-sitter/config.json` to list the repo's parent directory under `parser-directories`.

## Grammar design

Gherkin is line-oriented. The grammar is too: `source_file = repeat(_line)`, where each `_line` is one of tag/header/step/table/docstring/comment/description.

**The single most important trick:** the generic `_word` regex (`/[^\s|@#"'`<][^\s]*/`) competes with literal keywords at the tokenizer level. Tree-sitter picks the longest match, with literals winning ties. So:

- **Header keywords bake `:` into the token** — `token('Feature:')`, `token('Scenario:')`, `token(/Scenario[ \t]+Outline[ \t]*:/)`. This makes them tie with `_word` on length ("Feature:" = 8 chars) so the literal-wins rule fires.
- **Step keywords don't need the colon** — `Given`/`When`/`Then`/`And`/`But`/`*` tie with `_word` on length naturally.
- **Special tokens in step text** (`string`, `number`, `parameter`) use `token(prec(5, /…/))` to beat `_word`. Their first characters (`"`, `'`, digit, `<`) are excluded from the `_word` regex so ambiguity is avoided at the first char for quoted/bracketed forms.
- **`word: $ => $._word`** — this keyword-guard directive prevents `Given` from matching inside `Givenx`.

If you add a new construct that could collide with `description`/`step_text` words, check what `_word` does at its first character and either exclude that char from `_word` or rely on `prec` + length tie.

## Known grammar limitations (acceptable)

- A description line that starts with a step keyword (`Given is a word.`) parses as a step. Classic Gherkin limitation.
- Lines starting with `|`, `@`, `#` always take the table/tag/comment path even inside prose — don't start description lines with those.

## Shipping grammar changes to Zed

Zed's extension builder clones the grammar repo at a specific git rev. After any grammar or query change:

```sh
cd tree-sitter-gherkin
git add -A && git -c commit.gpgsign=false commit -m "..."
git rev-parse HEAD
```

Then update `rev = "..."` in the top-level `extension.toml` to that SHA and reinstall via `zed: install dev extension`. **`rev = "HEAD"` does not work** — Zed calls `git fetch <rev>` and needs a real commit.

Keep `tree-sitter-gherkin/queries/highlights.scm` and `languages/gherkin/highlights.scm` in sync. They cover the same captures; the former is for `tree-sitter highlight` locally, the latter is what Zed actually uses.

## Highlight capture notes

`@variable` renders as default foreground in many Zed themes — don't use it for constructs that should pop. `parameter` uses `(parameter) @embedded @constant` (Zed falls through to the second capture if the first isn't themed). Other safe scopes in use: `@keyword`, `@string`, `@number`, `@comment`, `@attribute`, `@punctuation.bracket`, `@punctuation.special`.

## Formatter development loop

From `crates/gherkin-fmt/`:

```sh
cargo test --quiet          # unit tests in src/main.rs
cargo run --quiet -- </tmp/x.feature   # stdin → stdout end-to-end
cargo install --path .      # reinstall to ~/.cargo/bin/gherkin-fmt
```

Same TDD discipline: add a test in the `#[cfg(test)] mod tests` block first, then implement.

Zed wiring (user's `settings.json`):

```json
"languages": {
  "Gherkin": {
    "format_on_save": "on",
    "formatter": {
      "external": { "command": "gherkin-fmt", "arguments": [] }
    }
  }
}
```

Zed pipes the buffer over stdin and replaces it with stdout. No extension-API hook exists for formatting — LSP or external is the only path.

## Formatter design

`format_gherkin` walks lines once with a small state machine (`has_rule`, `content_depth`, `in_docstring`, `pending_tags`) and streams into an `Emitter` that does blank-line normalization on the fly.

**Canonical depths** (2-space unit):
| construct | no `Rule:` | under `Rule:` |
|---|---|---|
| `Feature:` | 0 | — |
| `Rule:` | — | 1 |
| `Scenario:` / `Background:` / `Scenario Outline:` | 1 | 2 |
| steps, `Examples:`, description, comments | 2 | 3 |
| table rows, doc-string `"""` markers | 3 | 4 |

Depth is tracked by `content_depth`, which is set on every header. `Examples:` and steps emit at `content_depth`; tables and doc-string markers emit at `content_depth + 1`.

**Tags** are buffered in `pending_tags` until the next structural line; `flush_tags` emits them on one line at that line's depth, with single-space separators regardless of how the source had them grouped.

**Blank lines** are collapsed by the `Emitter`: leading blanks are dropped (`at_start`), runs are coalesced (`pending_blank`), and the final `\n` is guaranteed.

**Doc-string contents** are pushed via `push_verbatim` — never re-indented and never blank-collapsed. The opening/closing `"""` or ```` ``` ```` markers are aligned to `content_depth + 1` but the body is left untouched.

**Column alignment:**
- First row is always the header; excluded from numeric detection.
- A column is numeric-aligned iff every non-empty *body* cell parses as `f64`.
- Indent is passed in by the caller; `format_table` returns a `Vec<String>` (one per row, already indented).
- Escaped pipes `\|` stay inside cells (`split_cells` handles the `\\` escape).
- For a standalone table with no `Feature:` seen yet, indent falls back to `leading_indent(rows[0])` — useful for unit-testing tables in isolation.

**Not yet handled** (add a failing test first if you grow any of these): multi-byte display width (east-asian wide chars, emoji), line reflow / wrapping of long step text, consistent blank-line policy *between* scenarios (currently only collapses, never inserts).
