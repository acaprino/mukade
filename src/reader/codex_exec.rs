//! Codex Desktop "code mode" (spec L10, tables §5): almost every call of a Desktop rollout is one
//! custom tool `exec` whose input is a JavaScript script. A script that is exactly one call,
//!
//! ```text
//! text(await tools.exec_command({cmd: "ls", yield_time_ms: 1000}));
//! text(await tools.apply_patch("*** Begin Patch ..."));
//! ```
//!
//! with literal arguments only, is read as the call it wraps: `exec_command` with the object as its
//! arguments, `apply_patch` with the patch text as its input, which is how Codex CLI records the
//! same two calls. Anything else (several calls, variables, `Promise.allSettled`, `${…}` in a
//! template, another tool) is `None`: the caller keeps the `exec` call as it is, in text form.
//!
//! The parser accepts only what it can read without doubt: JSON values, plus the JavaScript
//! spellings a script uses for them (identifier keys, single-quoted and backtick strings, trailing
//! commas). It never evaluates anything.

use serde_json::{Map, Value};

/// The tools a one-call script may wrap.
const WRAPPED: [&str; 2] = ["exec_command", "apply_patch"];

/// The call a code-mode script is, when it is exactly one wrapped call with literal arguments.
pub fn wrapped_call(script: &str) -> Option<(String, Value)> {
    let mut p = Parser::new(script);
    p.blank();
    p.word("text")?;
    p.blank();
    p.symbol('(')?;
    p.blank();
    p.word("await")?;
    p.blank_required()?;
    p.word("tools")?;
    p.blank();
    p.symbol('.')?;
    p.blank();
    let name = p.identifier()?;
    if !WRAPPED.contains(&name.as_str()) {
        return None;
    }
    p.blank();
    p.symbol('(')?;
    p.blank();
    let input = p.value(0)?;
    p.blank();
    p.symbol(')')?;
    p.blank();
    p.symbol(')')?;
    p.blank();
    if p.peek() == Some(';') {
        p.next();
        p.blank();
    }
    if p.peek().is_some() {
        return None;
    }
    match (name.as_str(), &input) {
        ("exec_command", Value::Object(args)) if args.get("cmd").is_some_and(Value::is_string) => {}
        ("apply_patch", Value::String(_)) => {}
        _ => return None,
    }
    Some((name, input))
}

/// How deep objects and arrays may nest: a script nests a couple of levels at most.
const MAX_DEPTH: usize = 32;

struct Parser<'a> {
    rest: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Parser {
            rest: text.chars().peekable(),
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.rest.peek().copied()
    }

    fn next(&mut self) -> Option<char> {
        self.rest.next()
    }

    /// Whitespace and nothing else: a comment is not something this parser reads past.
    fn blank(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.next();
        }
    }

    fn blank_required(&mut self) -> Option<()> {
        if !self.peek().is_some_and(char::is_whitespace) {
            return None;
        }
        self.blank();
        Some(())
    }

    fn symbol(&mut self, expected: char) -> Option<()> {
        (self.next()? == expected).then_some(())
    }

    fn word(&mut self, expected: &str) -> Option<()> {
        (self.identifier()? == expected).then_some(())
    }

    fn identifier(&mut self) -> Option<String> {
        let mut out = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                out.push(c);
                self.next();
            } else {
                break;
            }
        }
        (!out.is_empty() && !out.starts_with(|c: char| c.is_ascii_digit())).then_some(out)
    }

    fn value(&mut self, depth: usize) -> Option<Value> {
        if depth > MAX_DEPTH {
            return None;
        }
        match self.peek()? {
            '{' => self.object(depth),
            '[' => self.array(depth),
            '"' | '\'' | '`' => self.string().map(Value::String),
            '-' | '0'..='9' => self.number(),
            _ => match self.identifier()?.as_str() {
                "true" => Some(Value::Bool(true)),
                "false" => Some(Value::Bool(false)),
                "null" => Some(Value::Null),
                // A variable, `undefined`, a call: not a literal.
                _ => None,
            },
        }
    }

    fn object(&mut self, depth: usize) -> Option<Value> {
        self.symbol('{')?;
        let mut out = Map::new();
        loop {
            self.blank();
            if self.peek()? == '}' {
                self.next();
                return Some(Value::Object(out));
            }
            let key = match self.peek()? {
                '"' | '\'' => self.string()?,
                _ => self.identifier()?,
            };
            self.blank();
            self.symbol(':')?;
            self.blank();
            let value = self.value(depth + 1)?;
            // A repeated key would mean "the last one wins" in JavaScript; refuse rather than pick.
            if out.insert(key, value).is_some() {
                return None;
            }
            self.blank();
            match self.next()? {
                ',' => continue,
                '}' => return Some(Value::Object(out)),
                _ => return None,
            }
        }
    }

    fn array(&mut self, depth: usize) -> Option<Value> {
        self.symbol('[')?;
        let mut out = Vec::new();
        loop {
            self.blank();
            if self.peek()? == ']' {
                self.next();
                return Some(Value::Array(out));
            }
            out.push(self.value(depth + 1)?);
            self.blank();
            match self.next()? {
                ',' => continue,
                ']' => return Some(Value::Array(out)),
                _ => return None,
            }
        }
    }

    fn string(&mut self) -> Option<String> {
        let quote = self.next()?;
        let mut out = String::new();
        loop {
            let c = self.next()?;
            if c == quote {
                return Some(out);
            }
            match c {
                // A template with a substitution is an expression, not a literal.
                '$' if quote == '`' && self.peek() == Some('{') => return None,
                // A plain string cannot hold a line break; a template can.
                '\n' | '\r' if quote != '`' => return None,
                '\\' => match self.next()? {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'v' => out.push('\u{b}'),
                    '0' if !self.peek().is_some_and(|c| c.is_ascii_digit()) => out.push('\0'),
                    'u' => out.push(self.unicode()?),
                    // A line continuation adds nothing.
                    '\n' => {}
                    c @ ('\\' | '\'' | '"' | '`' | '/' | '$') => out.push(c),
                    _ => return None,
                },
                c => out.push(c),
            }
        }
    }

    /// `\uXXXX` (a surrogate pair as two of them) or `\u{X…}`.
    fn unicode(&mut self) -> Option<char> {
        if self.peek() == Some('{') {
            self.next();
            let mut hex = String::new();
            while self.peek()? != '}' {
                hex.push(self.next()?);
            }
            self.next();
            return char::from_u32(u32::from_str_radix(&hex, 16).ok()?);
        }
        let high = self.four_hex()?;
        if (0xD800..0xDC00).contains(&high) {
            self.symbol('\\')?;
            self.symbol('u')?;
            let low = self.four_hex()?;
            if !(0xDC00..0xE000).contains(&low) {
                return None;
            }
            return char::from_u32(0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00));
        }
        char::from_u32(high)
    }

    fn four_hex(&mut self) -> Option<u32> {
        let hex: String = (0..4).map(|_| self.next()).collect::<Option<_>>()?;
        u32::from_str_radix(&hex, 16).ok()
    }

    /// A JSON number (JavaScript's `1_000`, hex and the like are refused).
    fn number(&mut self) -> Option<Value> {
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E') {
                text.push(c);
                self.next();
            } else {
                break;
            }
        }
        let value: Value = serde_json::from_str(&text).ok()?;
        value.is_number().then_some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_one_call_shell_script_is_that_call() {
        assert_eq!(
            wrapped_call(
                r#"text(await tools.exec_command({cmd: "git status", yield_time_ms: 1000}));"#
            ),
            Some((
                "exec_command".into(),
                json!({"cmd": "git status", "yield_time_ms": 1000})
            ))
        );
        assert_eq!(
            wrapped_call(
                "\n  text( await tools.exec_command({ 'cmd': 'ls -la', \"workdir\": `C:\\\\work`, max_output_tokens: 6000, }) )\n"
            ),
            Some((
                "exec_command".into(),
                json!({"cmd": "ls -la", "workdir": "C:\\work", "max_output_tokens": 6000})
            ))
        );
    }

    #[test]
    fn a_one_call_patch_script_is_that_call() {
        let patch = "*** Begin Patch\n*** Update File: a.txt\n@@\n-old\n+new\n*** End Patch";
        let script = format!(
            "text(await tools.apply_patch({}));",
            serde_json::to_string(patch).unwrap()
        );
        assert_eq!(
            wrapped_call(&script),
            Some(("apply_patch".into(), Value::String(patch.into())))
        );
        let template = format!("text(await tools.apply_patch(`{patch}`))");
        assert_eq!(
            wrapped_call(&template),
            Some(("apply_patch".into(), Value::String(patch.into())))
        );
    }

    #[test]
    fn strings_decode_their_escapes() {
        assert_eq!(
            wrapped_call(
                r#"text(await tools.exec_command({cmd: "echo \"a\tb\" \u00e9 \ud83d\ude00 \u{1F600}"}))"#
            ),
            Some((
                "exec_command".into(),
                json!({"cmd": "echo \"a\tb\" é 😀 😀"})
            ))
        );
    }

    #[test]
    fn anything_but_one_literal_call_is_left_alone() {
        for script in [
            // several calls
            r#"text(await tools.exec_command({cmd: "a"})); text(await tools.exec_command({cmd: "b"}));"#,
            r#"const r = await Promise.allSettled([tools.exec_command({cmd: "a"})]); text(r);"#,
            // variables and expressions
            r#"const c = "ls"; text(await tools.exec_command({cmd: c}));"#,
            r#"text(await tools.exec_command({cmd: "a" + "b"}));"#,
            r#"text(await tools.exec_command({cmd: `ls ${dir}`}));"#,
            r#"text(await tools.exec_command({cmd: "a", cmd: "b"}));"#,
            r#"text(await tools.exec_command({cmd: "ls", timeout: undefined}));"#,
            r#"text(await tools.exec_command({cmd: "ls", n: 0x10}));"#,
            // another tool, or none
            r#"text(await tools.view_image({path: "a.png"}));"#,
            r#"text(await tools.web__run({q: "x"}));"#,
            r#"text(await ALL_TOOLS.exec_command({cmd: "ls"}));"#,
            // the wrong argument shape
            r#"text(await tools.exec_command("ls"));"#,
            r#"text(await tools.exec_command({command: "ls"}));"#,
            r#"text(await tools.apply_patch({patch: "x"}));"#,
            // not awaited, not printed, or broken
            r#"text(tools.exec_command({cmd: "ls"}));"#,
            r#"await tools.exec_command({cmd: "ls"});"#,
            r#"text(await tools.exec_command({cmd: "ls"})"#,
            r#"text(await tools.exec_command({cmd: "ls"})) // done"#,
            r#"text(awaittools.exec_command({cmd: "ls"}))"#,
            "text(await tools.exec_command({cmd: \"two\nlines\"}))",
            "",
        ] {
            assert_eq!(wrapped_call(script), None, "{script}");
        }
    }

    #[test]
    fn nesting_has_a_limit() {
        let deep = format!(
            "text(await tools.exec_command({{cmd: \"x\", a: {}1{}}}))",
            "[".repeat(100),
            "]".repeat(100)
        );
        assert_eq!(wrapped_call(&deep), None);
    }
}
