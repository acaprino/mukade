//! Codex Desktop "code mode" (spec L10, tables §5): a tool call `exec` whose input is a script.
//!
//! Only one shape of script is read as the calls it makes: a sequence of statements
//! `text(await tools.<name>(<literal>));` and nothing else. The arguments are literals (objects,
//! arrays, strings, numbers, `true`, `false`, `null`); nothing is evaluated. Any other script is
//! left whole, and travels as one call in text form.

use crate::model::{CanonicalTool, Entry, Kind, ToolCall, ToolResult};
use serde_json::{Map, Number, Value};
use std::collections::HashMap;

/// Replace every `exec` call whose script and output can be read call by call with the calls it
/// made and their results (spec L10). The first call keeps the `exec` call's id, the n-th is
/// `<id>_<n>`: `_` is in every provider's id alphabet, so no writer has to rewrite it. Item 0 of
/// the output is Codex's header (`Script completed` …), then one `input_text` item per `text()`
/// call, in order. A script is left whole unless its shape and its output agree exactly; the
/// source records in `raw` are never touched.
/// `shells` holds, per `exec` call id, the shell declared before the call (L13).
pub(super) fn split_code_mode(
    entries: &mut [Entry],
    shells: &HashMap<String, Option<CanonicalTool>>,
) {
    for step in 0..entries.len() {
        let Kind::Step { calls, .. } = &entries[step].kind else {
            continue;
        };
        let execs: Vec<ParsedExec> = calls
            .iter()
            .enumerate()
            .filter(|(_, c)| c.source_name == "exec")
            .filter_map(|(i, c)| Some((i, c.id.clone(), parse_script(c.input.as_str()?)?)))
            .collect();
        // From the last, so the positions of the earlier ones stay valid.
        for (position, id, parsed) in execs.into_iter().rev() {
            let Some(output) = output_of(entries, step, &id, parsed.len()) else {
                continue;
            };
            let shell = shells.get(&id).cloned().flatten();
            let ids: Vec<String> = (1..=parsed.len())
                .map(|n| {
                    if n == 1 {
                        id.clone()
                    } else {
                        format!("{id}_{n}")
                    }
                })
                .collect();
            let results: Vec<ToolResult> = ids
                .iter()
                .zip(&output.items)
                .map(|(call_id, item)| ToolResult {
                    call_id: call_id.clone(),
                    content: crate::reader::json::result_content(&[super::output_item(item)]),
                    is_error: output.failed,
                    truncated_by_source: false,
                })
                .collect();
            if let Kind::Results { results: held } = &mut entries[output.entry].kind {
                held.splice(output.result..=output.result, results);
            }
            let calls: Vec<ToolCall> = ids
                .into_iter()
                .zip(parsed)
                .map(|(call_id, (name, input))| ToolCall {
                    id: call_id,
                    canonical: Some(super::canonical_call(&name, &input, shell.clone())),
                    source_name: name,
                    input,
                })
                .collect();
            if let Kind::Step { calls: held, .. } = &mut entries[step].kind {
                held.splice(position..=position, calls);
            }
        }
    }
}

/// An `exec` call's position in its step, its id, and the calls its script made.
type ParsedExec = (usize, String, Vec<(String, Value)>);

/// Where an `exec` call's result is, and its per-call output items.
struct Output {
    entry: usize,
    result: usize,
    items: Vec<Value>,
    failed: bool,
}

/// The output of the `exec` call `id`, when it holds exactly `calls` text items after a header
/// saying the script completed (or failed, for a one-call script).
fn output_of(entries: &[Entry], step: usize, id: &str, calls: usize) -> Option<Output> {
    let (entry, result) = entries
        .iter()
        .enumerate()
        .skip(step + 1)
        .find_map(|(e, entry)| match &entry.kind {
            Kind::Results { results } => {
                results.iter().position(|r| r.call_id == id).map(|r| (e, r))
            }
            _ => None,
        })?;
    let row = entries[entry].raw.iter().find(|row| {
        row.pointer("/payload/type").and_then(Value::as_str) == Some("custom_tool_call_output")
            && row.pointer("/payload/call_id").and_then(Value::as_str) == Some(id)
    })?;
    let items = row.pointer("/payload/output")?.as_array()?;
    let (header, rest) = items.split_first()?;
    let is_text = |item: &Value| item.get("type").and_then(Value::as_str) == Some("input_text");
    if !is_text(header) || rest.len() != calls || !rest.iter().all(is_text) {
        return None;
    }
    let header = header.get("text").and_then(Value::as_str)?;
    let failed = if header.starts_with("Script completed") {
        false
    } else if header.starts_with("Script failed") && calls == 1 {
        true
    } else {
        return None;
    };
    Some(Output {
        entry,
        result,
        items: rest.to_vec(),
        failed,
    })
}

/// The calls a script makes, in order, when it is only literal `text(await tools.X(...))`
/// statements; `None` for any other script. A call without arguments reads as `{}`, which is how
/// Codex records one.
pub(super) fn parse_script(script: &str) -> Option<Vec<(String, Value)>> {
    let mut p = Parser {
        chars: script.chars().collect(),
        at: 0,
        depth: 0,
    };
    let mut calls = Vec::new();
    p.skip_space();
    while !p.done() {
        calls.push(p.statement()?);
        p.skip_space();
    }
    (!calls.is_empty()).then_some(calls)
}

struct Parser {
    chars: Vec<char>,
    at: usize,
    /// How many objects and arrays the current literal is inside.
    depth: usize,
}

/// Deeper than any tool argument, and far from the stack's limit (serde_json's own cap).
const MAX_DEPTH: usize = 128;

impl Parser {
    fn done(&self) -> bool {
        self.at >= self.chars.len()
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += 1;
        Some(c)
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.at += 1;
        }
    }

    /// The exact text `word`, after optional whitespace.
    fn expect(&mut self, word: &str) -> Option<()> {
        self.skip_space();
        for w in word.chars() {
            (self.next()? == w).then_some(())?;
        }
        Some(())
    }

    /// `text(await tools.<name>(<literal>?))` with an optional `;`.
    fn statement(&mut self) -> Option<(String, Value)> {
        self.expect("text")?;
        self.expect("(")?;
        self.expect("await")?;
        // `await` must be followed by whitespace: `awaittools` is another identifier.
        if !self.peek().is_some_and(char::is_whitespace) {
            return None;
        }
        self.expect("tools")?;
        self.expect(".")?;
        let name = self.identifier()?;
        self.expect("(")?;
        self.skip_space();
        let args = if self.peek() == Some(')') {
            Value::Object(Map::new())
        } else {
            self.literal()?
        };
        self.expect(")")?;
        self.expect(")")?;
        self.skip_space();
        if self.peek() == Some(';') {
            self.at += 1;
        }
        Some((name, args))
    }

    fn identifier(&mut self) -> Option<String> {
        let start = self.at;
        let first = self.peek()?;
        if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
            return None;
        }
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
        {
            self.at += 1;
        }
        Some(self.chars[start..self.at].iter().collect())
    }

    fn literal(&mut self) -> Option<Value> {
        self.skip_space();
        match self.peek()? {
            '{' | '[' => {
                self.depth += 1;
                if self.depth > MAX_DEPTH {
                    return None;
                }
                let value = if self.peek()? == '{' {
                    self.object()
                } else {
                    self.array()
                };
                self.depth -= 1;
                value
            }
            '"' | '\'' => self.string().map(Value::String),
            '-' | '0'..='9' => self.number(),
            _ => match self.identifier()?.as_str() {
                "true" => Some(Value::Bool(true)),
                "false" => Some(Value::Bool(false)),
                "null" => Some(Value::Null),
                _ => None,
            },
        }
    }

    fn object(&mut self) -> Option<Value> {
        self.next();
        let mut map = Map::new();
        loop {
            self.skip_space();
            if self.peek()? == '}' {
                self.at += 1;
                return Some(Value::Object(map));
            }
            let key = match self.peek()? {
                '"' | '\'' => self.string()?,
                _ => self.identifier()?,
            };
            self.expect(":")?;
            let value = self.literal()?;
            map.insert(key, value);
            self.skip_space();
            match self.next()? {
                ',' => {}
                '}' => return Some(Value::Object(map)),
                _ => return None,
            }
        }
    }

    fn array(&mut self) -> Option<Value> {
        self.next();
        let mut items = Vec::new();
        loop {
            self.skip_space();
            if self.peek()? == ']' {
                self.at += 1;
                return Some(Value::Array(items));
            }
            items.push(self.literal()?);
            self.skip_space();
            match self.next()? {
                ',' => {}
                ']' => return Some(Value::Array(items)),
                _ => return None,
            }
        }
    }

    /// A `"…"` or `'…'` string with JavaScript's escapes. A raw line break refuses: only a
    /// template literal may hold one, and those are not read.
    fn string(&mut self) -> Option<String> {
        let quote = self.next()?;
        let mut out = String::new();
        loop {
            match self.next()? {
                c if c == quote => return Some(out),
                '\n' | '\r' => return None,
                '\\' => self.escape(&mut out)?,
                c => out.push(c),
            }
        }
    }

    fn escape(&mut self, out: &mut String) -> Option<()> {
        let c = match self.next()? {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'b' => '\u{8}',
            'f' => '\u{c}',
            'v' => '\u{b}',
            '0' if !self.peek().is_some_and(|c| c.is_ascii_digit()) => '\0',
            'x' => char::from_u32(self.hex(2)?)?,
            'u' => return self.unicode(out),
            // A line continuation adds nothing.
            '\n' => return Some(()),
            c @ ('\\' | '"' | '\'' | '/') => c,
            _ => return None,
        };
        out.push(c);
        Some(())
    }

    /// `\uHHHH` (a surrogate pair is two of them) or `\u{H…}`.
    fn unicode(&mut self, out: &mut String) -> Option<()> {
        if self.peek() == Some('{') {
            self.at += 1;
            let start = self.at;
            while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                self.at += 1;
            }
            let digits: String = self.chars[start..self.at].iter().collect();
            (self.next()? == '}').then_some(())?;
            out.push(char::from_u32(u32::from_str_radix(&digits, 16).ok()?)?);
            return Some(());
        }
        let first = self.hex(4)?;
        if (0xD800..0xDC00).contains(&first) {
            (self.next()? == '\\').then_some(())?;
            (self.next()? == 'u').then_some(())?;
            let second = self.hex(4)?;
            if !(0xDC00..0xE000).contains(&second) {
                return None;
            }
            out.push(char::from_u32(
                0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00),
            )?);
            return Some(());
        }
        out.push(char::from_u32(first)?);
        Some(())
    }

    fn hex(&mut self, digits: usize) -> Option<u32> {
        let mut value = 0;
        for _ in 0..digits {
            value = value * 16 + self.next()?.to_digit(16)?;
        }
        Some(value)
    }

    /// A decimal number. Without a fraction or an exponent it stays an integer.
    fn number(&mut self) -> Option<Value> {
        let start = self.at;
        if self.peek() == Some('-') {
            self.at += 1;
        }
        let digits = |p: &mut Parser| {
            let from = p.at;
            while p.peek().is_some_and(|c| c.is_ascii_digit()) {
                p.at += 1;
            }
            p.at > from
        };
        if !digits(self) {
            return None;
        }
        let mut float = false;
        if self.peek() == Some('.') {
            self.at += 1;
            float = true;
            digits(self).then_some(())?;
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            self.at += 1;
            float = true;
            if matches!(self.peek(), Some('+' | '-')) {
                self.at += 1;
            }
            digits(self).then_some(())?;
        }
        let text: String = self.chars[start..self.at].iter().collect();
        if !float {
            if let Ok(i) = text.parse::<i64>() {
                return Some(Value::Number(i.into()));
            }
            if let Ok(u) = text.parse::<u64>() {
                return Some(Value::Number(u.into()));
            }
        }
        Number::from_f64(text.parse::<f64>().ok()?).map(Value::Number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn calls(script: &str) -> Vec<(String, serde_json::Value)> {
        parse_script(script).unwrap_or_else(|| panic!("refused: {script}"))
    }

    #[test]
    fn one_statement_is_one_call() {
        assert_eq!(
            calls("text(await tools.exec_command({cmd:\"CMD-1\", yield_time_ms:1000}));"),
            vec![(
                "exec_command".to_string(),
                json!({"cmd": "CMD-1", "yield_time_ms": 1000})
            )]
        );
    }

    #[test]
    fn statements_in_sequence_are_calls_in_order() {
        let script = "text(await tools.exec_command({cmd:\"CMD-1\"}));\n\
                      text(await tools.apply_patch(\"*** Begin Patch\\n*** End Patch\"));\n  \
                      text(await tools.clock__curr_time({}))\n";
        assert_eq!(
            calls(script),
            vec![
                ("exec_command".to_string(), json!({"cmd": "CMD-1"})),
                (
                    "apply_patch".to_string(),
                    json!("*** Begin Patch\n*** End Patch")
                ),
                ("clock__curr_time".to_string(), json!({})),
            ]
        );
    }

    #[test]
    fn no_argument_is_an_empty_object() {
        assert_eq!(
            calls("text(await tools.a());"),
            vec![("a".to_string(), json!({}))]
        );
    }

    #[test]
    fn literals_of_every_kind() {
        let got =
            calls("text(await tools.t({a:'s', \"b\":-1.5e3, c:[1,true,null,{d:false}], e:[],}));");
        assert_eq!(
            got[0].1,
            json!({"a": "s", "b": -1500.0, "c": [1, true, null, {"d": false}], "e": []})
        );
        assert!(got[0].1["c"][0].is_i64(), "an integer stays an integer");
    }

    #[test]
    fn escapes_decode_like_javascript() {
        let got = calls(r#"text(await tools.t("a\nb\t\\\"q\" é \u{1F600} \' \x41 😀 \/"));"#);
        assert_eq!(
            got[0].1,
            json!("a\nb\t\\\"q\" \u{e9} \u{1F600} ' A \u{1F600} /")
        );
    }

    #[test]
    fn a_statement_boundary_inside_a_string_is_not_a_boundary() {
        let got =
            calls(r#"text(await tools.exec_command({cmd:"x));\ntext(await tools.y({}));"}));"#);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1["cmd"], "x));\ntext(await tools.y({}));");
    }

    /// A literal nested deeper than any tool argument is refused, not a stack overflow.
    #[test]
    fn deep_nesting_is_refused_not_a_crash() {
        let deep = format!(
            "text(await tools.a({}{}));",
            "[".repeat(100_000),
            "]".repeat(100_000)
        );
        assert_eq!(parse_script(&deep), None);
        let fine = format!(
            "text(await tools.a({}{}));",
            "[".repeat(100),
            "]".repeat(100)
        );
        assert!(parse_script(&fine).is_some());
    }

    #[test]
    fn anything_else_is_refused() {
        for script in [
            "const r = await Promise.allSettled([tools.exec_command({cmd:\"a\"})]);",
            "text(ALL_TOOLS.filter(x=>x));",
            "image(await tools.view_image({path:\"p\"}));",
            "text(await tools.exec_command({cmd:x}));",
            "text(await tools.exec_command({cmd:`a`}));",
            "text(await tools.a({}), 1);",
            "text(await tools.a({}, {}));",
            "// note\ntext(await tools.a({}));",
            "",
            "   \n",
            "text(await tools.a({}));  extra",
            "text(await tools.a({...b}));",
            "text(await tools.a({cmd:\"line\nbreak\"}));",
            "text(await tools.a(1 + 2));",
            "text(await tools.a(1e999));",
            "text(await tools.a({a:1 b:2}));",
            "text(await tools.a([1 2]));",
            "text(await tools.a(\"unterminated));",
        ] {
            assert_eq!(parse_script(script), None, "{script:?}");
        }
    }
}
