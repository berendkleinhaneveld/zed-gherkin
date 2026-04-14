use std::io::{self, Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = match Config::from_args(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("gherkin-fmt: {e}");
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("gherkin-fmt: reading stdin: {e}");
        return ExitCode::from(1);
    }
    let output = format_gherkin_with(&input, &config);
    if let Err(e) = io::stdout().write_all(output.as_bytes()) {
        eprintln!("gherkin-fmt: writing stdout: {e}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

const USAGE: &str = "\
usage: gherkin-fmt [options] < input.feature > output.feature

options:
  --tags-per-line    emit each @tag on its own line (default: join on one)
  -h, --help         show this help";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub tags_per_line: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { tags_per_line: false }
    }
}

impl Config {
    fn from_args(args: &[String]) -> Result<Self, String> {
        let mut cfg = Config::default();
        for arg in args {
            match arg.as_str() {
                "--tags-per-line" => cfg.tags_per_line = true,
                "-h" | "--help" => {
                    println!("{USAGE}");
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument: {other}")),
            }
        }
        Ok(cfg)
    }
}

const INDENT: &str = "  ";

/// Format a Gherkin document. In order, this:
///
/// 1. Re-indents every structural line to the canonical 2-space nesting:
///    `Feature:` at col 0, `Rule:` at 2, `Scenario:`/`Background:` at 2
///    (or 4 under a `Rule:`), steps and `Examples:` at 4 (or 6), tables
///    and doc-string markers one level deeper than that.
/// 2. Column-aligns every contiguous `|…|` table region — pipes line up,
///    numeric body columns right-align, text columns left-align.
/// 3. Buffers `@tag` lines and emits them at the indent of the header
///    they precede, joined on one line with single spaces.
/// 4. Collapses runs of blank lines to at most one, strips leading and
///    trailing blanks, and guarantees a single trailing `\n`.
/// 5. Leaves doc-string *contents* (lines between `"""` / ```` ``` ````
///    markers) verbatim — no re-indent, no blank-collapse.
pub fn format_gherkin(input: &str) -> String {
    format_gherkin_with(input, &Config::default())
}

pub fn format_gherkin_with(input: &str, config: &Config) -> String {
    let raw: Vec<&str> = input.split('\n').collect();
    // `split('\n')` on a trailing-newline file yields a spurious empty
    // element — drop it so we don't synthesize an extra blank line.
    let lines: &[&str] = if raw.last() == Some(&"") {
        &raw[..raw.len().saturating_sub(1)]
    } else {
        &raw[..]
    };

    let mut emitter = Emitter::new();
    let mut has_rule = false;
    let mut content_depth: usize = 0;
    let mut seen_header = false;
    let mut in_docstring = false;
    let mut pending_tags: Vec<String> = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];

        if in_docstring {
            if is_docstring_marker(line) {
                in_docstring = false;
                emitter.push(format!(
                    "{}{}",
                    INDENT.repeat(content_depth + 1),
                    line.trim()
                ));
            } else {
                emitter.push_verbatim(line.trim_end().to_string());
            }
            i += 1;
            continue;
        }

        if is_blank(line) {
            emitter.push_blank();
            i += 1;
            continue;
        }

        if is_tag(line) {
            pending_tags.push(line.trim().to_string());
            i += 1;
            continue;
        }

        if is_table(line) {
            let start = i;
            while i < lines.len() && is_table(lines[i]) {
                i += 1;
            }
            let indent = if seen_header {
                INDENT.repeat(content_depth + 1)
            } else {
                // Standalone table with no enclosing Feature — preserve
                // whatever the author already used.
                leading_indent(lines[start]).to_string()
            };
            flush_tags(&mut emitter, &mut pending_tags, &indent, config);
            let region: Vec<&str> = lines[start..i].to_vec();
            for row in format_table(&region, &indent) {
                emitter.push(row);
            }
            continue;
        }

        if is_docstring_marker(line) {
            let depth = content_depth + 1;
            let indent = INDENT.repeat(depth);
            flush_tags(&mut emitter, &mut pending_tags, &indent, config);
            emitter.push(format!("{}{}", indent, line.trim()));
            in_docstring = true;
            i += 1;
            continue;
        }

        // Structural / body line — compute the depth it should go at,
        // flush any pending tag block at the same depth, then emit.
        let depth = if is_feature(line) {
            0
        } else if is_rule(line) {
            1
        } else if is_background(line) || is_scenario(line) {
            if has_rule { 2 } else { 1 }
        } else if is_examples(line) || is_step(line) {
            content_depth
        } else {
            // Comments and free-text descriptions: indent to the current
            // child level of the enclosing block.
            content_depth
        };
        let indent_str = INDENT.repeat(depth);
        flush_tags(&mut emitter, &mut pending_tags, &indent_str, config);
        emitter.push(format!("{}{}", indent_str, line.trim()));

        // State transitions happen *after* emission.
        if is_feature(line) {
            has_rule = false;
            content_depth = 1;
            seen_header = true;
        } else if is_rule(line) {
            has_rule = true;
            content_depth = 2;
            seen_header = true;
        } else if is_background(line) || is_scenario(line) {
            content_depth = if has_rule { 3 } else { 2 };
            seen_header = true;
        }
        // Examples and Step don't change content_depth — table goes at +1.

        i += 1;
    }

    // Dangling tags at EOF (malformed file but don't drop them).
    if !pending_tags.is_empty() {
        let indent_str = INDENT.repeat(content_depth);
        flush_tags(&mut emitter, &mut pending_tags, &indent_str, config);
    }

    emitter.finish()
}

// ---------- streaming emitter with blank-line normalization ----------

struct Emitter {
    out: Vec<String>,
    // Blank lines *outside* doc-strings are collapsed: a run of N blanks
    // becomes one, leading blanks are dropped, and trailing blanks are
    // trimmed at finish(). Verbatim lines (doc-string content) bypass
    // this and reset the collapsing counter.
    pending_blank: bool,
    at_start: bool,
}

impl Emitter {
    fn new() -> Self {
        Self { out: Vec::new(), pending_blank: false, at_start: true }
    }

    fn push(&mut self, line: String) {
        if self.pending_blank && !self.at_start {
            self.out.push(String::new());
        }
        self.pending_blank = false;
        self.at_start = false;
        self.out.push(line);
    }

    fn push_verbatim(&mut self, line: String) {
        // Doc-string body — never dropped, never coalesced.
        if self.pending_blank && !self.at_start {
            self.out.push(String::new());
            self.pending_blank = false;
        }
        self.at_start = false;
        self.out.push(line);
    }

    fn push_blank(&mut self) {
        if !self.at_start {
            self.pending_blank = true;
        }
    }

    fn finish(self) -> String {
        // Trailing blanks already suppressed by `pending_blank` staying
        // unflushed at EOF. Always terminate with exactly one newline.
        let mut s = self.out.join("\n");
        if !s.is_empty() && !s.ends_with('\n') {
            s.push('\n');
        }
        if s.is_empty() {
            s.push('\n');
        }
        // drop any accidental trailing whitespace introduced by callers
        s
    }
}

// ---------- line classification ----------

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

fn is_tag(line: &str) -> bool {
    line.trim_start().starts_with('@')
}

fn is_table(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

fn is_comment(line: &str) -> bool {
    line.trim_start().starts_with('#')
}

fn is_docstring_marker(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("\"\"\"") || t.starts_with("```")
}

fn is_feature(line: &str) -> bool {
    line.trim_start().starts_with("Feature:")
}

fn is_rule(line: &str) -> bool {
    line.trim_start().starts_with("Rule:")
}

fn is_background(line: &str) -> bool {
    line.trim_start().starts_with("Background:")
}

fn is_scenario(line: &str) -> bool {
    let t = line.trim_start();
    if t.starts_with("Scenario:") || t.starts_with("Example:") {
        return true;
    }
    if let Some(rest) = t.strip_prefix("Scenario ") {
        let after = rest.trim_start();
        return after.starts_with("Outline:") || after.starts_with("Template:");
    }
    false
}

fn is_examples(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("Examples:") || t.starts_with("Scenarios:")
}

fn is_step(line: &str) -> bool {
    let t = line.trim_start();
    for kw in ["Given", "When", "Then", "And", "But"] {
        if let Some(rest) = t.strip_prefix(kw) {
            if rest.is_empty()
                || rest.starts_with(' ')
                || rest.starts_with('\t')
            {
                return true;
            }
        }
    }
    t == "*" || t.starts_with("* ") || t.starts_with("*\t")
}

fn leading_indent(line: &str) -> &str {
    let end = line.len() - line.trim_start().len();
    &line[..end]
}

// ---------- tag flushing ----------

fn flush_tags(
    out: &mut Emitter,
    pending: &mut Vec<String>,
    indent: &str,
    config: &Config,
) {
    if pending.is_empty() {
        return;
    }
    // Accept one-per-line OR multiple-per-line source formats. Collect
    // every individual tag, then re-emit either all on one line
    // (default) or one per line (--tags-per-line).
    let tags: Vec<String> = pending
        .iter()
        .flat_map(|s| s.split_whitespace().map(String::from).collect::<Vec<_>>())
        .collect();
    pending.clear();
    if config.tags_per_line {
        for tag in tags {
            out.push(format!("{}{}", indent, tag));
        }
    } else {
        out.push(format!("{}{}", indent, tags.join(" ")));
    }
}

// ---------- table alignment (unchanged logic, explicit indent) ----------

fn split_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let inner = trimmed
        .strip_prefix('|')
        .and_then(|s| s.strip_suffix('|'))
        .unwrap_or(trimmed);
    let mut cells = Vec::new();
    let mut buf = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                buf.push(c);
                buf.push(next);
                chars.next();
                continue;
            }
        }
        if c == '|' {
            cells.push(buf.trim().to_string());
            buf.clear();
        } else {
            buf.push(c);
        }
    }
    cells.push(buf.trim().to_string());
    cells
}

fn display_width(s: &str) -> usize {
    s.chars().count()
}

fn is_numeric(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty() && t.parse::<f64>().is_ok()
}

/// Render a block of table rows, one row per returned String, each
/// prefixed with `indent`. Column widths are computed across the block;
/// body columns (everything past row 0) whose non-empty cells all parse
/// as numbers are right-aligned.
fn format_table(rows: &[&str], indent: &str) -> Vec<String> {
    let parsed: Vec<Vec<String>> = rows.iter().map(|r| split_cells(r)).collect();
    let col_count = parsed.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut widths = vec![0usize; col_count];
    for row in &parsed {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(display_width(cell));
        }
    }
    let mut numeric = vec![parsed.len() > 1; col_count];
    for row in parsed.iter().skip(1) {
        for (i, cell) in row.iter().enumerate() {
            if !cell.is_empty() && !is_numeric(cell) {
                numeric[i] = false;
            }
        }
    }
    let mut result = Vec::with_capacity(parsed.len());
    for row in &parsed {
        let mut line = String::new();
        line.push_str(indent);
        line.push('|');
        for (ci, cell) in row.iter().enumerate() {
            let w = widths[ci];
            let pad = w - display_width(cell);
            line.push(' ');
            if numeric[ci] {
                for _ in 0..pad {
                    line.push(' ');
                }
                line.push_str(cell);
            } else {
                line.push_str(cell);
                for _ in 0..pad {
                    line.push(' ');
                }
            }
            line.push(' ');
            line.push('|');
        }
        result.push(line);
    }
    result
}

// silence unused-warning for is_comment — kept for future blank-line
// rules that want to treat comments specially.
#[allow(dead_code)]
fn _keep_is_comment_live(line: &str) -> bool {
    is_comment(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(s: &str) -> String {
        format_gherkin(s)
    }

    fn fmt_split(s: &str) -> String {
        format_gherkin_with(s, &Config { tags_per_line: true })
    }

    // ---------- table alignment ----------

    #[test]
    fn leaves_canonical_layout_untouched() {
        let src = "Feature: f\n  Scenario: s\n    Given a thing\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn strips_trailing_whitespace() {
        let src = "Feature: f   \n  Scenario: s  \n";
        let want = "Feature: f\n  Scenario: s\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn aligns_numeric_table_right() {
        let src = "\
Feature: f
  Scenario Outline: s
    Examples:
      | start | eat | left |
      | 12 | 5 | 7 |
      | 20 | 5 | 15 |
";
        let want = "\
Feature: f
  Scenario Outline: s
    Examples:
      | start | eat | left |
      |    12 |   5 |    7 |
      |    20 |   5 |   15 |
";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn aligns_text_table_left_without_feature_context() {
        let src = "  | name | role |\n  | Alice | admin |\n  | Bob | user |\n";
        let want = "  | name  | role  |\n  | Alice | admin |\n  | Bob   | user  |\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn mixed_numeric_and_text_columns() {
        let src = "| name | count |\n| Alice | 2 |\n| Bob | 10 |\n";
        let want = "| name  | count |\n| Alice |     2 |\n| Bob   |    10 |\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn escaped_pipe_stays_inside_cell() {
        let src = "| a\\|b | c |\n| 1 | 2 |\n";
        let want = "| a\\|b | c |\n|    1 | 2 |\n";
        assert_eq!(fmt(src), want);
    }

    // ---------- step indentation ----------

    #[test]
    fn reindents_under_feature() {
        let src = "\
Feature: f
Scenario: s
Given a
When b
Then c
";
        let want = "\
Feature: f
  Scenario: s
    Given a
    When b
    Then c
";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn reindents_under_rule() {
        let src = "\
Feature: f
Rule: r
Scenario: s
Given a
";
        let want = "\
Feature: f
  Rule: r
    Scenario: s
      Given a
";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn reindents_examples_and_its_table() {
        let src = "\
Feature: f
Scenario Outline: s
Given <a>
Examples:
| a |
| 1 |
";
        let want = "\
Feature: f
  Scenario Outline: s
    Given <a>
    Examples:
      | a |
      | 1 |
";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn all_step_keywords_reindented() {
        let src = "\
Feature: f
Scenario: s
    Given a
   When b
     Then c
 And d
       But e
  * f
";
        let want = "\
Feature: f
  Scenario: s
    Given a
    When b
    Then c
    And d
    But e
    * f
";
        assert_eq!(fmt(src), want);
    }

    // ---------- tags ----------

    #[test]
    fn feature_level_tags_at_depth_zero() {
        let src = "@smoke @wip\nFeature: f\n  Scenario: s\n    Given a\n";
        let want = "@smoke @wip\nFeature: f\n  Scenario: s\n    Given a\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn scenario_tags_indented_to_scenario() {
        let src = "\
Feature: f
@slow
Scenario: s
Given a
";
        let want = "\
Feature: f
  @slow
  Scenario: s
    Given a
";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn tags_per_line_mode_splits_joined_input() {
        let src = "@a @b @c\nFeature: f\n  Scenario: s\n    Given x\n";
        let want = "\
@a
@b
@c
Feature: f
  Scenario: s
    Given x
";
        assert_eq!(fmt_split(src), want);
    }

    #[test]
    fn tags_per_line_mode_preserves_already_split_input() {
        let src = "\
@a
@b
Feature: f
  @slow
  @wip
  Scenario: s
    Given x
";
        assert_eq!(fmt_split(src), src);
    }

    #[test]
    fn default_mode_joins_regardless_of_input_layout() {
        let src = "\
@a
@b
Feature: f
@slow
@wip
Scenario: s
Given x
";
        let want = "\
@a @b
Feature: f
  @slow @wip
  Scenario: s
    Given x
";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn multiple_tag_lines_joined() {
        let src = "\
Feature: f
@a
@b @c
Scenario: s
Given x
";
        let want = "\
Feature: f
  @a @b @c
  Scenario: s
    Given x
";
        assert_eq!(fmt(src), want);
    }

    // ---------- blank line normalization ----------

    #[test]
    fn collapses_runs_of_blank_lines() {
        let src = "\
Feature: f


  Scenario: s



    Given a
";
        let want = "\
Feature: f

  Scenario: s

    Given a
";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn strips_leading_and_trailing_blanks() {
        let src = "\n\n\nFeature: f\n  Scenario: s\n    Given a\n\n\n";
        let want = "Feature: f\n  Scenario: s\n    Given a\n";
        assert_eq!(fmt(src), want);
    }

    // ---------- docstrings ----------

    #[test]
    fn docstring_markers_indented_content_preserved() {
        let src = "\
Feature: f
Scenario: s
Given a payload
\"\"\"
  hello

  world
\"\"\"
";
        let want = "\
Feature: f
  Scenario: s
    Given a payload
      \"\"\"
  hello

  world
      \"\"\"
";
        assert_eq!(fmt(src), want);
    }

    // ---------- combined ----------

    #[test]
    fn cucumber_example_end_to_end() {
        // Input omits the `Feature:` line, but the formatter assumes an
        // implicit feature and indents the scenario to 2 spaces — the
        // canonical layout you'd get after adding a `Feature:` above.
        let src = "\
Scenario Outline: eating
  Given there are <start> cucumbers
  When I eat <eat> cucumbers
  Then I should have <left> cucumbers

  Examples:
    | start | eat | left |
    | 12 | 5 | 7 |
    | 20 | 5 | 15 |
";
        // Build `want` without a `"\` continuation — it would strip the
        // leading indent from the first line and hide the 2-space scenario
        // indent we actually care about.
        let want = concat!(
            "  Scenario Outline: eating\n",
            "    Given there are <start> cucumbers\n",
            "    When I eat <eat> cucumbers\n",
            "    Then I should have <left> cucumbers\n",
            "\n",
            "    Examples:\n",
            "      | start | eat | left |\n",
            "      |    12 |   5 |    7 |\n",
            "      |    20 |   5 |   15 |\n",
        );
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn real_world_with_feature() {
        let src = "\
Feature: Cukes

Scenario Outline: eating
Given there are <start> cucumbers
When I eat <eat> cucumbers
Then I should have <left> cucumbers

Examples:
| start | eat | left |
| 12 | 5 | 7 |
| 20 | 5 | 15 |
";
        let want = "\
Feature: Cukes

  Scenario Outline: eating
    Given there are <start> cucumbers
    When I eat <eat> cucumbers
    Then I should have <left> cucumbers

    Examples:
      | start | eat | left |
      |    12 |   5 |    7 |
      |    20 |   5 |   15 |
";
        assert_eq!(fmt(src), want);
    }
}
