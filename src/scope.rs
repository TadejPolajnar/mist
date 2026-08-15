use regex::Regex;
use std::collections::BTreeSet;

pub fn suffixed(class: &str, unit: &str) -> String {
    format!("{}--{}", class, unit)
}

pub fn scope_style(css: &str, unit: &str) -> (String, BTreeSet<String>) {
    let css = crate::tailwind::strip_comments(css);
    let mut names = BTreeSet::new();
    let out = scope_block(&css, unit, &mut names);
    (out, names)
}

fn scope_block(css: &str, unit: &str, names: &mut BTreeSet<String>) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let close = open + matching_brace(&rest[open..]);
        let selector = &rest[..open];
        let body = &rest[open + 1..close];
        let head = selector.trim_start();
        if head.starts_with("@keyframes") || head.starts_with("@font-face") {
            out.push_str(selector);
            out.push('{');
            out.push_str(body);
        } else if head.starts_with('@') {
            out.push_str(selector);
            out.push('{');
            out.push_str(&scope_block(body, unit, names));
        } else {
            out.push_str(&scope_selector(selector, unit, names));
            out.push('{');
            out.push_str(body);
        }
        out.push('}');
        rest = if close < rest.len() { &rest[close + 1..] } else { "" };
    }
    out.push_str(rest);
    out
}

fn matching_brace(s: &str) -> usize {
    let mut depth = 0;
    let mut quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    s.len()
}

fn scope_selector(selector: &str, unit: &str, names: &mut BTreeSet<String>) -> String {
    let re = Regex::new(r"\.([A-Za-z0-9_-]+)").unwrap();
    let apply = |text: &str, names: &mut BTreeSet<String>| {
        re.replace_all(text, |caps: &regex::Captures| {
            let name = caps[1].to_string();
            let s = format!(".{}", suffixed(&name, unit));
            names.insert(name);
            s
        })
        .to_string()
    };
    let mut out = String::new();
    let mut rest = selector;
    while let Some(pos) = rest.find(['\'', '"']) {
        let quote = rest[pos..].chars().next().unwrap();
        out.push_str(&apply(&rest[..pos], names));
        let end = rest[pos + 1..].find(quote).map(|i| pos + 1 + i + 1).unwrap_or(rest.len());
        out.push_str(&rest[pos..end]);
        rest = &rest[end..];
    }
    out.push_str(&apply(rest, names));
    out
}

pub fn scope_wxml(wxml: &str, names: &BTreeSet<String>, unit: &str) -> String {
    let mut out = String::new();
    let mut rest = wxml;
    loop {
        let hit = [" class=\"", " hover-class=\"", " placeholder-class=\""]
            .iter()
            .filter_map(|a| rest.find(a).map(|i| (i, a.len())))
            .min();
        let Some((pos, attr_len)) = hit else { break };
        let value_start = pos + attr_len;
        let Some(value_len) = rest[value_start..].find('"') else { break };
        out.push_str(&rest[..value_start]);
        out.push_str(&scope_class_value(&rest[value_start..value_start + value_len], names, unit));
        out.push('"');
        rest = &rest[value_start + value_len + 1..];
    }
    out.push_str(rest);
    out
}

fn scope_class_value(value: &str, names: &BTreeSet<String>, unit: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(open) = rest.find("{{") {
        let close = open + mustache_end(&rest[open..]);
        out.push_str(&scope_plain_tokens(&rest[..open], names, unit));
        out.push_str(&scope_quoted_tokens(&rest[open..close], names, unit));
        rest = &rest[close..];
    }
    out.push_str(&scope_plain_tokens(rest, names, unit));
    out
}

fn mustache_end(s: &str) -> usize {
    let mut quote: Option<char> = None;
    let bytes = s.as_bytes();
    for (i, c) in s.char_indices() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '}' if bytes.get(i + 1) == Some(&b'}') => return i + 2,
            _ => {}
        }
    }
    s.len()
}

fn scope_plain_tokens(text: &str, names: &BTreeSet<String>, unit: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find(|c: char| !c.is_whitespace()) {
        out.push_str(&rest[..start]);
        let end = rest[start..]
            .find(char::is_whitespace)
            .map(|i| start + i)
            .unwrap_or(rest.len());
        let token = &rest[start..end];
        if names.contains(token) {
            out.push_str(&suffixed(token, unit));
        } else {
            out.push_str(token);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

pub fn scope_class_expr(expr: &str, names: &BTreeSet<String>, unit: &str) -> String {
    let mut out = String::new();
    let mut rest = expr;
    while let Some(pos) = rest.find(['\'', '"', '`']) {
        let quote = rest[pos..].chars().next().unwrap();
        out.push_str(&rest[..pos + 1]);
        rest = &rest[pos + 1..];
        let Some(end) = rest.find(quote) else { break };
        let inner = &rest[..end];
        if quote == '`' {
            out.push_str(&scope_template_literal(inner, names, unit));
        } else {
            out.push_str(&scope_plain_tokens(inner, names, unit));
        }
        out.push(quote);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

fn scope_template_literal(inner: &str, names: &BTreeSet<String>, unit: &str) -> String {
    let mut out = String::new();
    let mut rest = inner;
    while let Some(open) = rest.find("${") {
        out.push_str(&scope_plain_tokens(&rest[..open], names, unit));
        let after = &rest[open..];
        let close = interp_end(after);
        out.push_str(&after[..close]);
        rest = &after[close..];
    }
    out.push_str(&scope_plain_tokens(rest, names, unit));
    out
}

fn interp_end(s: &str) -> usize {
    let mut depth = 0;
    let mut quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' | '`' => quote = Some(c),
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
    }
    s.len()
}

fn scope_quoted_tokens(expr: &str, names: &BTreeSet<String>, unit: &str) -> String {
    let mut out = String::new();
    let mut rest = expr;
    while let Some(open) = rest.find('\'') {
        let Some(len) = rest[open + 1..].find('\'') else { break };
        out.push_str(&rest[..open + 1]);
        out.push_str(&scope_plain_tokens(&rest[open + 1..open + 1 + len], names, unit));
        out.push('\'');
        rest = &rest[open + 1 + len + 1..];
    }
    out.push_str(rest);
    out
}
