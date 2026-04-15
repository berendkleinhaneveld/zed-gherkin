use regex::Regex;

pub fn expression_to_regex(expr: &str) -> Result<Regex, regex::Error> {
    let mut out = String::from("^");
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '{' {
            if let Some(rel_end) = chars[i + 1..].iter().position(|&x| x == '}') {
                let end = i + 1 + rel_end;
                let name: String = chars[i + 1..end].iter().collect();
                out.push_str(parameter_regex(&name));
                i = end + 1;
                continue;
            }
            out.push_str(&regex::escape("{"));
            i += 1;
            continue;
        }
        if c == '(' {
            if let Some(rel_end) = chars[i + 1..].iter().position(|&x| x == ')') {
                let end = i + 1 + rel_end;
                let inside: String = chars[i + 1..end].iter().collect();
                out.push_str("(?:");
                out.push_str(&regex::escape(&inside));
                out.push_str(")?");
                i = end + 1;
                continue;
            }
            out.push_str(&regex::escape("("));
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            out.push(c);
            i += 1;
            continue;
        }
        let mut j = i;
        while j < chars.len() {
            let cj = chars[j];
            if cj.is_whitespace() || cj == '{' || cj == '(' {
                break;
            }
            j += 1;
        }
        let token: String = chars[i..j].iter().collect();
        if token.contains('/') && token.split('/').all(|p| !p.is_empty()) {
            out.push_str("(?:");
            for (k, p) in token.split('/').enumerate() {
                if k > 0 {
                    out.push('|');
                }
                out.push_str(&regex::escape(p));
            }
            out.push(')');
        } else {
            out.push_str(&regex::escape(&token));
        }
        i = j;
    }
    out.push('$');
    Regex::new(&out)
}

fn parameter_regex(name: &str) -> &'static str {
    match name {
        "string" => r#"(?:"[^"]*"|'[^']*')"#,
        "int" => r"-?\d+",
        "float" => r"-?\d+(?:\.\d+)?",
        "word" => r"\S+",
        "" => r".+?",
        _ => r".+?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn re(expr: &str) -> Regex {
        expression_to_regex(expr).unwrap()
    }

    #[test]
    fn literal_matches_exactly() {
        let r = re("I have a cat");
        assert!(r.is_match("I have a cat"));
        assert!(!r.is_match("I have a cat."));
        assert!(!r.is_match("you have a cat"));
    }

    #[test]
    fn anchoring_rejects_substring_matches() {
        let r = re("foo");
        assert!(!r.is_match("foobar"));
        assert!(!r.is_match("barfoo"));
    }

    #[test]
    fn escapes_regex_specials() {
        let r = re("price is $5.00");
        assert!(r.is_match("price is $5.00"));
        assert!(!r.is_match("price is $5X00"));
    }

    #[test]
    fn param_string_double_and_single() {
        let r = re("I say {string}");
        assert!(r.is_match(r#"I say "hello""#));
        assert!(r.is_match("I say 'hello'"));
        assert!(!r.is_match("I say hello"));
    }

    #[test]
    fn param_int() {
        let r = re("{int} cukes");
        assert!(r.is_match("42 cukes"));
        assert!(r.is_match("-5 cukes"));
        assert!(!r.is_match("3.14 cukes"));
    }

    #[test]
    fn param_float_accepts_int_and_float() {
        let r = re("{float} meters");
        assert!(r.is_match("3.14 meters"));
        assert!(r.is_match("42 meters"));
        assert!(r.is_match("-0.5 meters"));
        assert!(!r.is_match("abc meters"));
    }

    #[test]
    fn param_word_single_token_only() {
        let r = re("hi {word}");
        assert!(r.is_match("hi world"));
        assert!(!r.is_match("hi two words"));
    }

    #[test]
    fn param_anonymous_matches_anything_nonempty() {
        let r = re("value: {}");
        assert!(r.is_match("value: anything"));
        assert!(r.is_match("value: 42"));
    }

    #[test]
    fn optional_group() {
        let r = re("I have {int} cucumber(s)");
        assert!(r.is_match("I have 1 cucumber"));
        assert!(r.is_match("I have 5 cucumbers"));
        assert!(!r.is_match("I have 1 cukes"));
    }

    #[test]
    fn alternation_slash_two_way() {
        let r = re("I eat apple/pear");
        assert!(r.is_match("I eat apple"));
        assert!(r.is_match("I eat pear"));
        assert!(!r.is_match("I eat banana"));
    }

    #[test]
    fn alternation_three_way() {
        let r = re("color is red/green/blue");
        assert!(r.is_match("color is red"));
        assert!(r.is_match("color is green"));
        assert!(r.is_match("color is blue"));
    }

    #[test]
    fn leading_slash_is_literal_not_alternation() {
        let r = re("path /foo");
        assert!(r.is_match("path /foo"));
        assert!(!r.is_match("path foo"));
    }

    #[test]
    fn unknown_parameter_type_falls_back_to_wildcard() {
        let r = re("I have {customType}");
        assert!(r.is_match("I have anything"));
        assert!(r.is_match("I have 42"));
    }

    #[test]
    fn multiple_parameters() {
        let r = re("I transfer {int} from {string} to {string}");
        assert!(r.is_match(r#"I transfer 100 from "alice" to "bob""#));
        assert!(!r.is_match("I transfer 100 from alice to bob"));
    }
}
