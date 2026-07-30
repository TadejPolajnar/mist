use crate::template::{Attr, AttrValue, Node};

/// WXML/WXSS-safe class name: `md:flex` → `md_flex`, `w-[32px]` → `w-_32px_`,
pub fn sanitize(class: &str) -> String {
    class.chars().map(sanitize_char).collect()
}

pub fn sanitize_char(c: char) -> char {
    match c {
        ':' | '/' | '[' | ']' | '.' | '#' | '%' | '&' | '(' | ')' | ',' | '\'' => '_',
        other => other,
    }
}

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
            let sanitized: Vec<String> = content.split_whitespace().map(sanitize).collect();
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

fn collect_from_attr(attr: &Attr, out: &mut Vec<String>) {
    match &attr.value {
        AttrValue::Static(s) => out.extend(s.split_whitespace().map(String::from)),
        AttrValue::Expr(e) => {
            let mut chars = e.chars().peekable();
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
                    out.extend(content.split_whitespace().map(String::from));
                }
            }
        }
        AttrValue::Bare => {}
    }
}

