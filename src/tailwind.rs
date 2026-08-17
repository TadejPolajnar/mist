use crate::template::{Attr, AttrValue, Node};

/// WXML/WXSS-safe class name: `md:flex` → `md_flex`, `w-[32px]` → `w-_32px_`,
/// `bg-black/50` → `bg-black_50`
pub fn sanitize(class: &str) -> String {
    class.chars().map(sanitize_char).collect()
}

pub fn sanitize_char(c: char) -> char {
    match c {
        ':' | '/' | '[' | ']' | '.' | '#' | '%' | '&' | '(' | ')' | ',' | '\'' => '_',
        other => other,
    }
}

/// Sanitize class tokens inside the quoted string literals of a class expression.
pub fn sanitize_class_expr(expr: &str) -> String {
    let mut out = String::new();
    let mut chars = expr.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\'' || c == '"' {
            let quote = c;
            let mut content = String::new();
            for c2 in chars.by_ref() {
                if c2 == quote {
                    break;
                }
                content.push(c2);
            }
            let sanitized: Vec<String> = content.split_whitespace().map(|t| sanitize(t)).collect();
            // preserve leading/trailing spaces that matter for concatenation
            let lead = if content.starts_with(' ') { " " } else { "" };
            let trail = if content.ends_with(' ') && !content.trim().is_empty() { " " } else { "" };
            out.push(quote);
            out.push_str(lead);
            out.push_str(&sanitized.join(" "));
            out.push_str(trail);
            out.push(quote);
        } else {
            out.push(c);
        }
    }
    out
}

/// Class selectors (`.foo`) hand-written in a user `<style>` block's
/// selector positions, e.g. `.card { ... } .press:hover { ... }` → `["card",
/// "press"]`. Scans only selector text (outside declaration bodies) so
/// declaration values like `0.5px` are never mistaken for a class token —
/// the char before `.` must be a boundary and the first char after `.` must
/// not be a digit. Conditional group rules (`@media`, `@supports`, …) are
/// transparent — selectors nested inside are still scanned — while
/// `@keyframes`/`@font-face`/regular rule bodies are opaque.
pub fn harvest_style_classes(css: &str) -> Vec<String> {
    let css = strip_comments(css);
    let mut out = Vec::new();
    // true at each open-block level whose contents should still be scanned
    // for selectors (top level, or inside a conditional group rule)
    let mut transparent_stack: Vec<bool> = Vec::new();
    let mut selector = String::new();
    for c in css.chars() {
        let scanning = transparent_stack.iter().all(|t| *t);
        match c {
            '{' => {
                if scanning {
                    scan_selector_classes(&selector, &mut out);
                }
                let trimmed = selector.trim_start();
                let is_conditional_group = trimmed.starts_with("@media")
                    || trimmed.starts_with("@supports")
                    || trimmed.starts_with("@document")
                    || trimmed.starts_with("@container")
                    || trimmed.starts_with("@layer");
                transparent_stack.push(scanning && is_conditional_group);
                selector.clear();
            }
            '}' => {
                transparent_stack.pop();
                selector.clear();
            }
            _ if scanning => selector.push(c),
            _ => {}
        }
    }
    out
}

pub(crate) fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut chars = css.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            while let Some(c2) = chars.next() {
                if c2 == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn scan_selector_classes(selector: &str, out: &mut Vec<String>) {
    let chars: Vec<char> = selector.chars().collect();
    let mut i = 0;
    let mut prev_was_class_char = false;
    while i < chars.len() {
        if chars[i] == '.' {
            let boundary = i == 0
                || prev_was_class_char
                || matches!(chars[i - 1], ' ' | '\t' | '\n' | '\r' | ',' | '>' | '+' | '~' | '(' | ':');
            let next_ok = chars.get(i + 1).is_some_and(|c| c.is_alphabetic() || *c == '_' || *c == '-');
            if boundary && next_ok {
                let start = i + 1;
                let mut end = start;
                while end < chars.len()
                    && (chars[end].is_alphanumeric() || chars[end] == '_' || chars[end] == '-')
                {
                    end += 1;
                }
                out.push(chars[start..end].iter().collect());
                i = end;
                prev_was_class_char = true;
                continue;
            }
        }
        prev_was_class_char = false;
        i += 1;
    }
}

/// All class tokens used in the template (raw, unsanitized).
pub fn extract_classes(nodes: &[Node]) -> Vec<String> {
    let mut out = Vec::new();
    walk(nodes, &mut out);
    out.sort();
    out.dedup();
    out
}

fn walk(nodes: &[Node], out: &mut Vec<String>) {
    for node in nodes {
        match node {
            Node::Element { attrs, children, .. } => {
                for attr in attrs {
                    if attr.name == "class" {
                        collect_from_attr(attr, out);
                    } else if attr.name == "class:list" {
                        if let AttrValue::Expr(e) = &attr.value {
                            collect_class_list(e, out);
                        }
                    }
                }
                walk(children, out);
            }
            Node::For { children, .. } => walk(children, out),
            Node::If { children, else_children, .. } => {
                walk(children, out);
                walk(else_children, out);
            }
            _ => {}
        }
    }
}

fn collect_class_list(expr: &str, out: &mut Vec<String>) {
    let t = expr.trim();
    let Some(inner) = t.strip_prefix('[').and_then(|r| r.strip_suffix(']')) else { return };
    for item in crate::wxml::split_top_level_commas(inner) {
        let item = item.trim();
        if item.starts_with('{') && item.ends_with('}') {
            for entry in crate::wxml::split_top_level_commas(&item[1..item.len() - 1]) {
                let entry = entry.trim();
                if let Some(colon) = crate::wxml::top_level_colon(entry) {
                    let key = entry[..colon].trim().trim_matches('\'').trim_matches('"');
                    out.extend(key.split_whitespace().map(String::from));
                }
            }
        } else {
            let attr = Attr { name: "class".into(), value: AttrValue::Expr(item.to_string()) };
            collect_from_attr(&attr, out);
        }
    }
}

fn collect_from_attr(attr: &Attr, out: &mut Vec<String>) {
    match &attr.value {
        AttrValue::Static(s) => out.extend(s.split_whitespace().map(String::from)),
        AttrValue::Expr(e) => {
            let chars: Vec<char> = e.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                let c = chars[i];
                if c == '\'' || c == '"' {
                    let start = i + 1;
                    let mut j = start;
                    while j < chars.len() && chars[j] != c {
                        j += 1;
                    }
                    let mut k = i;
                    while k > 0 && chars[k - 1] == ' ' {
                        k -= 1;
                    }
                    let compared_before = k > 0 && chars[k - 1] == '=';
                    let mut n = j + 1;
                    while n < chars.len() && chars[n] == ' ' {
                        n += 1;
                    }
                    let compared_after = n < chars.len() && (chars[n] == '=' || chars[n] == '!');
                    if !compared_before && !compared_after {
                        let content: String = chars[start..j].iter().collect();
                        out.extend(content.split_whitespace().map(String::from));
                    }
                    i = j + 1;
                    continue;
                }
                i += 1;
            }
        }
        AttrValue::Bare => {}
    }
}

