//! Bounded data-only Mustache renderer for export templates.
use anyhow::{Result, anyhow, bail};
use serde_json::Value;

const MAX_SOURCE: usize = 64 * 1024;
const MAX_OUTPUT: usize = 4 * 1024 * 1024;
const MAX_DEPTH: usize = 32;
const MAX_RENDER_WORK: usize = 1_000_000;

#[derive(Debug)]
enum Node {
    Text(String),
    Variable {
        name: String,
        escaped: bool,
        pos: usize,
    },
    Section {
        name: String,
        inverted: bool,
        children: Vec<Node>,
    },
}
#[derive(Debug)]
enum Token {
    Text(String),
    Skip,
    Variable(String, bool, usize),
    Open(String, bool, usize),
    Close(String, usize),
}

pub fn validate(source: &str) -> Result<()> {
    parse(source).map(|_| ())
}
pub fn render(source: &str, context: &Value) -> Result<String> {
    let nodes = parse(source)?;
    let mut out = String::new();
    let mut remaining_work = MAX_RENDER_WORK;
    render_nodes(&nodes, &mut vec![context], &mut out, 0, &mut remaining_work)?;
    Ok(out)
}
fn parse(source: &str) -> Result<Vec<Node>> {
    if source.len() > MAX_SOURCE {
        bail!("template exceeds 64 KiB");
    }
    let tokens = lex(source)?;
    let (nodes, index, closing) = parse_nodes(&tokens, 0, 0)?;
    if let Some(name) = closing {
        bail!("unexpected closing section {name:?}");
    }
    if index != tokens.len() {
        bail!("invalid template near token {}", index + 1);
    }
    Ok(nodes)
}
fn lex(source: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut pos = 0;
    let mut open = "{{".to_owned();
    let mut close = "}}".to_owned();
    while pos < source.len() {
        let Some(rel) = source[pos..].find(&open) else {
            if pos < source.len() {
                tokens.push(Token::Text(source[pos..].to_owned()));
            }
            break;
        };
        let start = pos + rel;
        if start > pos {
            tokens.push(Token::Text(source[pos..start].to_owned()));
        }
        let triple = open == "{{" && source[start..].starts_with("{{{");
        let (tag_close, content_start) = if triple {
            ("}}}", start + 3)
        } else {
            (close.as_str(), start + open.len())
        };
        let end_rel = source[content_start..]
            .find(tag_close)
            .ok_or_else(|| anyhow!("unclosed tag at byte {start}"))?;
        let end = content_start + end_rel;
        let raw = source[content_start..end].trim();
        if raw.is_empty() {
            bail!("empty tag at byte {start}");
        }
        let next = end + tag_close.len();
        let delimiter_change = raw.starts_with('=') && raw.ends_with('=');
        if delimiter_change {
            if raw.len() < 2 {
                bail!("invalid delimiter change at byte {start}");
            }
            let spec = raw[1..raw.len() - 1].trim();
            let parts: Vec<_> = spec.split_whitespace().collect();
            if parts.len() != 2 {
                bail!("invalid delimiter change at byte {start}");
            }
            open = parts[0].to_owned();
            close = parts[1].to_owned();
            if open == close {
                bail!("opening and closing delimiters must differ at byte {start}");
            }
        }
        let (kind, name) = if delimiter_change {
            ('=', "")
        } else if triple {
            ('&', raw)
        } else {
            let mut chars = raw.chars();
            let c = chars.next().unwrap();
            if c == '<' || c == '$' {
                bail!("template inheritance and blocks are unsupported at byte {start}");
            }
            if "#^/!>&".contains(c) {
                (c, chars.as_str().trim())
            } else {
                ('v', raw)
            }
        };
        if name.is_empty() && kind != '!' && kind != '=' {
            bail!("empty tag at byte {start}");
        }
        if kind == '>' {
            bail!("partials are unsupported at byte {start}");
        }
        if kind == '&' || kind == 'v' {
            tokens.push(Token::Variable(name.to_owned(), kind == 'v', start));
        } else if kind == '#' || kind == '^' {
            tokens.push(Token::Open(name.to_owned(), kind == '^', start));
        } else if kind == '/' {
            tokens.push(Token::Close(name.to_owned(), start));
        } else {
            tokens.push(Token::Skip);
        }
        let line_start = source[..start].rfind('\n').map_or(0, |i| i + 1);
        let prefix = &source[line_start..start];
        let line_end = source[next..].find('\n').map(|i| next + i);
        let mut suffix = line_end.map_or(&source[next..], |i| &source[next..i]);
        if let Some(without_cr) = suffix.strip_suffix('\r') {
            suffix = without_cr;
        }
        let standalone = matches!(kind, '#' | '^' | '/' | '!' | '=')
            && prefix.bytes().all(|b| b == b' ' || b == b'\t')
            && suffix.bytes().all(|b| b == b' ' || b == b'\t');
        if standalone {
            if let Some(Token::Text(text)) = tokens
                .iter_mut()
                .rev()
                .find(|t| matches!(t, Token::Text(_)))
                && text.ends_with(prefix)
            {
                text.truncate(text.len() - prefix.len());
            }
            pos = line_end.map_or(source.len(), |i| i + 1);
        } else {
            pos = next;
        }
    }
    Ok(tokens)
}
fn parse_nodes(
    tokens: &[Token],
    mut i: usize,
    depth: usize,
) -> Result<(Vec<Node>, usize, Option<String>)> {
    if depth > MAX_DEPTH {
        bail!("template nesting exceeds 32 levels");
    }
    let mut nodes = Vec::new();
    while i < tokens.len() {
        match &tokens[i] {
            Token::Text(s) => nodes.push(Node::Text(s.clone())),
            Token::Skip => {}
            Token::Variable(name, escaped, pos) => nodes.push(Node::Variable {
                name: name.clone(),
                escaped: *escaped,
                pos: *pos,
            }),
            Token::Close(name, pos) => {
                return Ok((nodes, i + 1, Some(format!("{name}\u{1f}{pos}"))));
            }
            Token::Open(name, inverted, pos) => {
                let (children, next, closing) = parse_nodes(tokens, i + 1, depth + 1)?;
                let Some(closing) = closing else {
                    bail!("unclosed section {name:?} at byte {pos}");
                };
                let mut split = closing.split('\u{1f}');
                let close_name = split.next().unwrap_or_default();
                if close_name != name {
                    bail!("mismatched section {name:?} closed by {close_name:?} at byte {pos}");
                }
                nodes.push(Node::Section {
                    name: name.clone(),
                    inverted: *inverted,
                    children,
                });
                i = next;
                continue;
            }
        }
        i += 1;
    }
    Ok((nodes, i, None))
}
fn render_nodes(
    nodes: &[Node],
    stack: &mut Vec<&Value>,
    out: &mut String,
    depth: usize,
    remaining_work: &mut usize,
) -> Result<()> {
    *remaining_work = remaining_work
        .checked_sub(1)
        .ok_or_else(|| anyhow!("template rendering exceeds the work limit"))?;
    if depth > MAX_DEPTH {
        bail!("template rendering exceeds 32 levels");
    }
    for node in nodes {
        *remaining_work = remaining_work
            .checked_sub(1)
            .ok_or_else(|| anyhow!("template rendering exceeds the work limit"))?;
        match node {
            Node::Text(text) => push_bounded(out, text)?,
            Node::Variable { name, escaped, pos } => {
                if let Some(value) = lookup(name, stack) {
                    let rendered = stringify(value);
                    if *escaped {
                        push_bounded(out, &escape_html(&rendered))
                    } else {
                        push_bounded(out, &rendered)
                    }
                    .map_err(|e| anyhow!("{e} (tag at byte {pos})"))?;
                }
            }
            Node::Section {
                name,
                inverted,
                children,
            } => {
                let value = lookup(name, stack);
                if *inverted {
                    if !value.is_some_and(truthy) {
                        render_nodes(children, stack, out, depth + 1, remaining_work)?;
                    }
                } else if let Some(value) = value.filter(|v| truthy(v)) {
                    match value {
                        Value::Array(items) => {
                            for item in items {
                                stack.push(item);
                                render_nodes(children, stack, out, depth + 1, remaining_work)?;
                                stack.pop();
                            }
                        }
                        Value::Object(_) => {
                            stack.push(value);
                            render_nodes(children, stack, out, depth + 1, remaining_work)?;
                            stack.pop();
                        }
                        Value::Bool(true) => {
                            render_nodes(children, stack, out, depth + 1, remaining_work)?
                        }
                        Value::String(_) | Value::Number(_) => {
                            stack.push(value);
                            render_nodes(children, stack, out, depth + 1, remaining_work)?;
                            stack.pop();
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(())
}
fn lookup<'a>(name: &str, stack: &[&'a Value]) -> Option<&'a Value> {
    let name = name.trim();
    if name == "." {
        return stack.last().copied();
    }
    let mut parts = name.split('.');
    let first = parts.next()?;
    let mut found = None;
    for ctx in stack.iter().rev() {
        if let Some(v) = ctx.get(first) {
            found = Some(v);
            break;
        }
    }
    let mut value = found?;
    for part in parts {
        value = value.get(part)?;
    }
    Some(value)
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Array(a) => !a.is_empty(),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn stringify(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        _ => v.to_string(),
    }
}
fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '/' => out.push_str("&#x2F;"),
            '\u{60}' => out.push_str("&#x60;"),
            '=' => out.push_str("&#x3D;"),
            _ => out.push(c),
        }
    }
    out
}
fn push_bounded(out: &mut String, text: &str) -> Result<()> {
    if out
        .len()
        .checked_add(text.len())
        .is_none_or(|n| n > MAX_OUTPUT)
    {
        bail!("rendered template exceeds 4 MiB");
    }
    out.push_str(text);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn nested_sections_parent_lookup_and_escaping() {
        let ctx = json!({"title":"A <b> &","groups":[{"name":"One","items":[{"label":"x"}]},{"name":"Two","items":[]}],"shown":true});
        let src = "{{title}}|{{{title}}}{{#groups}}[{{name}}:{{#items}}{{name}}{{label}}{{/items}}{{^items}}empty{{/items}}]{{/groups}}{{#shown}}!{{/shown}}";
        assert_eq!(
            render(src, &ctx).unwrap(),
            "A &lt;b&gt; &amp;|A <b> &[One:Onex][Two:empty]!"
        );
    }
    #[test]
    fn syntax_errors_and_standalone_lines() {
        validate("{{#x}}\n  {{! comment}}\n{{/x}}\n").unwrap();
        assert_eq!(
            render("a\n  {{#x}}\ny\n  {{/x}}\nz", &json!({"x":true})).unwrap(),
            "a\ny\nz"
        );
        assert_eq!(
            render("a\r\n  {{#x}}\r\ny\r\n  {{/x}}\r\nz", &json!({"x":true})).unwrap(),
            "a\r\ny\r\nz"
        );
        assert!(validate("{{#x}}").is_err());
        assert!(validate("{{#x}}{{/y}}").is_err());
        assert!(validate("{{> unsafe}}").is_err());
        assert!(validate("{{< layout}}").is_err());
        assert!(validate("{{$slot}}").is_err());
    }
    #[test]
    fn false_values_render_only_inverted_sections() {
        for value in [json!(""), Value::Null, json!(false), json!([])] {
            let context = json!({"value": value});
            assert_eq!(
                render(
                    "{{#value}}normal{{/value}}{{^value}}inverted{{/value}}",
                    &context
                )
                .unwrap(),
                "inverted"
            );
        }
        assert_eq!(
            render(
                "{{#value}}normal{{/value}}{{^value}}inverted{{/value}}",
                &json!({"value":0})
            )
            .unwrap(),
            "normal"
        );
    }
    #[test]
    fn dotted_names_and_delimiter_changes() {
        assert_eq!(
            render("{{a.b}} {{= <% %> =}}<%a.b%>", &json!({"a":{"b":"ok"}})).unwrap(),
            "ok ok"
        );
    }
    #[test]
    fn malformed_delimiter_changes_return_errors() {
        for source in ["{{=}}", "{{ = }}", "{{==}}", "{{= <% %> =}}<%=%>"] {
            assert!(
                validate(source)
                    .unwrap_err()
                    .to_string()
                    .contains("invalid delimiter change")
            );
            assert!(
                render(source, &json!({}))
                    .unwrap_err()
                    .to_string()
                    .contains("invalid delimiter change")
            );
        }
    }
    #[test]
    fn source_and_output_limits_are_enforced() {
        assert!(validate(&"x".repeat(MAX_SOURCE + 1)).is_err());
        let src = "{{#items}}{{.}}{{/items}}";
        let ctx =
            json!({"items":[ "x".repeat(MAX_OUTPUT / 2 + 1), "y".repeat(MAX_OUTPUT / 2 + 1) ]});
        assert!(render(src, &ctx).is_err());
    }

    #[test]
    fn nested_loops_are_bounded_even_when_they_produce_no_output() {
        let source = format!("{}{}", "{{#items}}".repeat(5), "{{/items}}".repeat(5));
        let context = json!({"items": vec![0; 100]});
        let error = render(&source, &context).unwrap_err();
        assert!(error.to_string().contains("work limit"));
    }
}
