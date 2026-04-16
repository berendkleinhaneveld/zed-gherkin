use crate::expression::expression_to_regex;
use ignore::WalkBuilder;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    Given,
    When,
    Then,
    Any,
}

#[derive(Debug)]
pub struct StepDef {
    pub path: PathBuf,
    pub line: u32,
    pub col_start: u32,
    pub col_end: u32,
    #[allow(dead_code)]
    pub kind: StepKind,
    #[allow(dead_code)]
    pub expression: String,
    pub regex: Regex,
}

#[derive(Debug)]
pub struct StepCall {
    pub path: PathBuf,
    pub line: u32,
    pub col_start: u32,
    pub col_end: u32,
    #[allow(dead_code)]
    pub keyword: String,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct Index {
    pub defs: Vec<StepDef>,
    pub calls: Vec<StepCall>,
}

impl Index {
    pub fn build(root: &Path) -> Self {
        let mut index = Index::default();
        for entry in WalkBuilder::new(root).require_git(false).build().flatten() {
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let path = entry.path();
            let Ok(content) = fs::read_to_string(path) else {
                continue;
            };
            index.scan_file(path, &content);
        }
        index
    }

    pub fn scan_file(&mut self, path: &Path, content: &str) {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "feature" => scan_feature(path, content, &mut self.calls),
            "py" => scan_python(path, content, &mut self.defs),
            "js" | "ts" | "mjs" | "cjs" => scan_javascript(path, content, &mut self.defs),
            _ => {}
        }
    }

    pub fn drop_file(&mut self, path: &Path) {
        self.defs.retain(|d| d.path != path);
        self.calls.retain(|c| c.path != path);
    }
}

fn parse_kind(kw: &str) -> StepKind {
    match kw.to_ascii_lowercase().as_str() {
        "given" => StepKind::Given,
        "when" => StepKind::When,
        "then" => StepKind::Then,
        _ => StepKind::Any,
    }
}

static FEATURE_RE: OnceLock<Regex> = OnceLock::new();
static PYTHON_RE: OnceLock<Regex> = OnceLock::new();
static JS_RE: OnceLock<Regex> = OnceLock::new();

fn feature_re() -> &'static Regex {
    FEATURE_RE.get_or_init(|| Regex::new(r"^\s*(Given|When|Then|And|But|\*)\s+(.+?)\s*$").unwrap())
}

fn python_re() -> &'static Regex {
    PYTHON_RE.get_or_init(|| {
        Regex::new(
            r#"^\s*@(given|when|then|step)\s*\(\s*(?:\w+\.\w+\s*\(\s*)?[ur]?(?:"([^"]*)"|'([^']*)')"#,
        )
        .unwrap()
    })
}

fn js_re() -> &'static Regex {
    JS_RE.get_or_init(|| {
        Regex::new(
            r#"^\s*(Given|When|Then|And|But|defineStep)\s*\(\s*(?:"([^"]*)"|'([^']*)'|`([^`]*)`)"#,
        )
        .unwrap()
    })
}

fn scan_feature(path: &Path, content: &str, out: &mut Vec<StepCall>) {
    let re = feature_re();
    for (line_idx, line) in content.lines().enumerate() {
        if let Some(caps) = re.captures(line) {
            let keyword = caps.get(1).unwrap().as_str().to_string();
            let text_m = caps.get(2).unwrap();
            out.push(StepCall {
                path: path.to_path_buf(),
                line: line_idx as u32,
                col_start: text_m.start() as u32,
                col_end: text_m.end() as u32,
                keyword,
                text: text_m.as_str().to_string(),
            });
        }
    }
}

fn scan_python(path: &Path, content: &str, out: &mut Vec<StepDef>) {
    let re = python_re();
    for (line_idx, line) in content.lines().enumerate() {
        if let Some(caps) = re.captures(line) {
            let kw = caps.get(1).unwrap().as_str();
            let expr_m = caps.get(2).or_else(|| caps.get(3)).unwrap();
            push_def(out, path, line_idx, expr_m, kw);
        }
    }
}

fn scan_javascript(path: &Path, content: &str, out: &mut Vec<StepDef>) {
    let re = js_re();
    for (line_idx, line) in content.lines().enumerate() {
        if let Some(caps) = re.captures(line) {
            let kw = caps.get(1).unwrap().as_str();
            let expr_m = caps
                .get(2)
                .or_else(|| caps.get(3))
                .or_else(|| caps.get(4))
                .unwrap();
            push_def(out, path, line_idx, expr_m, kw);
        }
    }
}

fn push_def(
    out: &mut Vec<StepDef>,
    path: &Path,
    line_idx: usize,
    expr_m: regex::Match,
    kw: &str,
) {
    let expr = expr_m.as_str().to_string();
    if let Ok(rx) = expression_to_regex(&expr) {
        out.push(StepDef {
            path: path.to_path_buf(),
            line: line_idx as u32,
            col_start: expr_m.start() as u32,
            col_end: expr_m.end() as u32,
            kind: parse_kind(kw),
            expression: expr,
            regex: rx,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn feature_extracts_all_step_keywords() {
        let mut index = Index::default();
        let content = "\
Feature: demo
  Scenario: one
    Given I have 5 cukes
    When I eat them
    Then they are gone
    And something else
    But not this
    * also this
";
        index.scan_file(&p("demo.feature"), content);
        assert_eq!(index.calls.len(), 6);
        assert_eq!(index.calls[0].keyword, "Given");
        assert_eq!(index.calls[0].text, "I have 5 cukes");
        assert_eq!(index.calls[1].keyword, "When");
        assert_eq!(index.calls[2].keyword, "Then");
        assert_eq!(index.calls[3].keyword, "And");
        assert_eq!(index.calls[4].keyword, "But");
        assert_eq!(index.calls[5].keyword, "*");
    }

    #[test]
    fn feature_skips_non_step_lines() {
        let mut index = Index::default();
        let content = "\
Feature: demo
  # a comment
  @tag
  Scenario: one
    Given foo
";
        index.scan_file(&p("x.feature"), content);
        assert_eq!(index.calls.len(), 1);
        assert_eq!(index.calls[0].text, "foo");
    }

    #[test]
    fn python_behave_decorators_double_quote() {
        let mut index = Index::default();
        let content = r#"
from behave import given, when, then

@given("I have {int} cukes")
def step_impl(context, count):
    pass

@when("I eat {int}")
def _(context, n): pass

@then("they are gone")
def _(context): pass
"#;
        index.scan_file(&p("steps.py"), content);
        assert_eq!(index.defs.len(), 3);
        assert_eq!(index.defs[0].expression, "I have {int} cukes");
        assert_eq!(index.defs[0].kind, StepKind::Given);
        assert_eq!(index.defs[1].kind, StepKind::When);
        assert_eq!(index.defs[2].kind, StepKind::Then);
    }

    #[test]
    fn python_single_quote_and_unicode_prefix() {
        let mut index = Index::default();
        let content = r#"
@given('single quotes work')
def _(c): pass

@when(u"unicode prefix")
def _(c): pass

@step("catchall")
def _(c): pass
"#;
        index.scan_file(&p("steps.py"), content);
        assert_eq!(index.defs.len(), 3);
        assert_eq!(index.defs[0].expression, "single quotes work");
        assert_eq!(index.defs[1].expression, "unicode prefix");
        assert_eq!(index.defs[2].expression, "catchall");
        assert_eq!(index.defs[2].kind, StepKind::Any);
    }

    #[test]
    fn python_commented_out_decorator_is_ignored() {
        let mut index = Index::default();
        let content = r#"
# @given("should not match")
# def _(c): pass

@given("should match")
def _(c): pass
"#;
        index.scan_file(&p("steps.py"), content);
        assert_eq!(index.defs.len(), 1);
        assert_eq!(index.defs[0].expression, "should match");
    }

    #[test]
    fn python_pytest_bdd_parsers_wrapper() {
        let mut index = Index::default();
        let content = r#"
from pytest_bdd import given, parsers

@given(parsers.parse("I have {int} cukes"))
def _(): pass

@given(parsers.cfparse('another pattern'))
def _(): pass
"#;
        index.scan_file(&p("steps.py"), content);
        assert_eq!(index.defs.len(), 2);
        assert_eq!(index.defs[0].expression, "I have {int} cukes");
        assert_eq!(index.defs[1].expression, "another pattern");
    }

    #[test]
    fn javascript_cucumber_js_all_quote_styles() {
        let mut index = Index::default();
        let content = r#"
const { Given, When, Then, And, But } = require('@cucumber/cucumber');

Given('I have {int} cukes', function (n) {});
When("I eat them", (ctx) => {});
Then(`they are gone`, async () => {});
And('something', () => {});
But('not this', () => {});
"#;
        index.scan_file(&p("steps.js"), content);
        assert_eq!(index.defs.len(), 5);
        assert_eq!(index.defs[0].expression, "I have {int} cukes");
        assert_eq!(index.defs[1].expression, "I eat them");
        assert_eq!(index.defs[2].expression, "they are gone");
        assert_eq!(index.defs[3].expression, "something");
        assert_eq!(index.defs[4].expression, "not this");
    }

    #[test]
    fn typescript_works_same_as_js() {
        let mut index = Index::default();
        let content = r#"
import { Given } from '@cucumber/cucumber';
Given('ts works', (): void => {});
"#;
        index.scan_file(&p("steps.ts"), content);
        assert_eq!(index.defs.len(), 1);
        assert_eq!(index.defs[0].expression, "ts works");
    }

    #[test]
    fn expression_regex_matches_step_call() {
        let mut index = Index::default();
        index.scan_file(
            &p("a.py"),
            r#"@given("I have {int} cukes")
def _(c, n): pass
"#,
        );
        index.scan_file(
            &p("b.feature"),
            "Feature: x\n  Scenario: y\n    Given I have 5 cukes\n",
        );
        assert_eq!(index.defs.len(), 1);
        assert_eq!(index.calls.len(), 1);
        assert!(index.defs[0].regex.is_match(&index.calls[0].text));
    }

    #[test]
    fn drop_file_removes_entries() {
        let mut index = Index::default();
        index.scan_file(&p("a.py"), r#"@given("x")
def _(): pass
"#);
        index.scan_file(&p("b.feature"), "  Given x\n");
        assert_eq!(index.defs.len(), 1);
        assert_eq!(index.calls.len(), 1);
        index.drop_file(&p("a.py"));
        assert_eq!(index.defs.len(), 0);
        assert_eq!(index.calls.len(), 1);
    }

    #[test]
    fn build_walks_directory_tree() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.feature"), "  Given hello\n").unwrap();
        fs::create_dir_all(dir.path().join("steps")).unwrap();
        fs::write(
            dir.path().join("steps/impl.py"),
            r#"@given("hello")
def _(c): pass
"#,
        )
        .unwrap();

        let index = Index::build(dir.path());
        assert_eq!(index.defs.len(), 1);
        assert_eq!(index.calls.len(), 1);
    }

    #[test]
    fn build_respects_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".gitignore"), "ignored/\n").unwrap();
        fs::create_dir_all(dir.path().join("ignored")).unwrap();
        fs::write(
            dir.path().join("ignored/steps.py"),
            r#"@given("should not appear")
def _(): pass
"#,
        )
        .unwrap();
        fs::write(dir.path().join("visible.py"), r#"@given("visible")
def _(): pass
"#).unwrap();

        let index = Index::build(dir.path());
        assert_eq!(index.defs.len(), 1);
        assert_eq!(index.defs[0].expression, "visible");
    }
}
