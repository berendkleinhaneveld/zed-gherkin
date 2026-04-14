; Header keywords — Feature:, Rule:, Background:, Scenario:, Scenario Outline:, Examples:
(feature_keyword) @keyword
(rule_keyword) @keyword
(background_keyword) @keyword
(scenario_keyword) @keyword
(examples_keyword) @keyword

; Step keywords — Given / When / Then / And / But / *
(step_keyword) @keyword

; String, number, and parameter placeholders inside step text
(string) @string
(number) @number
(parameter) @embedded @constant

; Tags — @smoke, @wip
(tag) @attribute

; Tables
"|" @punctuation.bracket
(table_cell) @string

; Doc strings
(docstring) @string
"\"\"\"" @punctuation.special
"```" @punctuation.special

; Inline comments (# ...)
(comment) @comment

; Everything that is not a feature/scenario name, header, step, tag, or
; table is "description" free-text — render it like a comment.
(description) @comment
