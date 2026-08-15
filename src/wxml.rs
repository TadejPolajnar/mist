use crate::template::{Attr, AttrValue, Node};
use regex::Regex;

pub struct Handler {
    pub name: String,
    pub target: String,
    pub args: Vec<String>,
    pub from_detail: bool,
}

pub struct WxmlOutput {
    pub wxml: String,
    pub handlers: Vec<Handler>,
    pub used_components: Vec<String>,
    pub used_inline: Vec<String>,
    /// state names two-way bound via `value:bind={x}`/`checked:bind={x}` (need `__vb_<x>` handlers)
    pub vbinds: Vec<String>,
    /// page-scope template expressions hoisted into generated deriveds `_h<i>`
    pub hoisted: Vec<String>,
    /// indices into `hoisted` whose expressions produce class names (from
    /// `class`/`class:list` attrs) — `<style scoped>` rewrites their literals
    pub class_hoists: Vec<usize>,
    /// per-item hoists: loops whose items gain computed `_c<i>` fields
    pub for_hoists: Vec<ForHoist>,
    /// tags neither a native WeChat element, a web alias, nor a registered
    /// component/inline use — (tag, did-you-mean suggestion), deduped, M1019
    pub unknown_tags: Vec<(String, Option<String>)>,
}

/// A `wx:for` list rewritten to a generated derived that maps computed fields
/// (`_c0`, `_c1`, …) onto each item, so calls like `fmt(t.ts)` work per item.
pub struct ForHoist {
    pub name: String,
    pub list: String,
    pub param: String,
    pub key: Option<String>,
    pub fields: Vec<String>,
}

struct Ctx {
    strip_value: Regex,
    handlers: Vec<Handler>,
    components: Vec<String>,
    inline: Vec<String>,
    used_components: Vec<String>,
    used_inline: Vec<String>,
    vbinds: Vec<String>,
    hoisted: Vec<String>,
    class_hoists: Vec<usize>,
    for_hoists: Vec<ForHoist>,
    /// active expr → replacement rewrites while emitting a hoisted loop body
    rewrites: Vec<(String, String)>,
    call_re: Regex,
    loop_params: Vec<String>,
    unknown_tags: Vec<(String, Option<String>)>,
}

pub fn emit(
    nodes: &[Node],
    reactive_names: &[String],
    components: &[String],
    inline: &[String],
) -> Result<WxmlOutput, String> {
    let pattern = if reactive_names.is_empty() {
        r"\b__none__\.value".to_string()
    } else {
        format!(r"\b({})\.value", reactive_names.join("|"))
    };
    let mut ctx = Ctx {
        strip_value: Regex::new(&pattern).map_err(|e| e.to_string())?,
        handlers: Vec::new(),
        components: components.to_vec(),
        inline: inline.to_vec(),
        used_components: Vec::new(),
        used_inline: Vec::new(),
        vbinds: Vec::new(),
        hoisted: Vec::new(),
        class_hoists: Vec::new(),
        for_hoists: Vec::new(),
        rewrites: Vec::new(),
        call_re: Regex::new(r"[A-Za-z0-9_\]]\s*\(").map_err(|e| e.to_string())?,
        loop_params: Vec::new(),
        unknown_tags: Vec::new(),
    };
    let mut out = String::new();
    emit_nodes(nodes, &mut ctx, &mut out, 0)?;
    Ok(WxmlOutput {
        wxml: out,
        handlers: ctx.handlers,
        used_components: ctx.used_components,
        used_inline: ctx.used_inline,
        vbinds: ctx.vbinds,
        hoisted: ctx.hoisted,
        class_hoists: ctx.class_hoists,
        for_hoists: ctx.for_hoists,
        unknown_tags: ctx.unknown_tags,
    })
}

/// `TodoItem` → `todo-item`
pub fn kebab(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn emit_nodes(nodes: &[Node], ctx: &mut Ctx, out: &mut String, indent: usize) -> Result<(), String> {
    for node in nodes {
        emit_node(node, ctx, out, indent)?;
    }
    Ok(())
}

fn emit_node(node: &Node, ctx: &mut Ctx, out: &mut String, indent: usize) -> Result<(), String> {
    let pad = "  ".repeat(indent);
    match node {
        Node::Text(t) => {
            out.push_str(&pad);
            out.push_str(t);
            out.push('\n');
        }
        Node::Expr(e) => {
            out.push_str(&pad);
            let b = ctx.hoist_or_bind(e)?;
            out.push_str(&format!("{{{{{}}}}}", b));
            out.push('\n');
        }
        Node::Element { tag, attrs, children } => {
            if ctx.inline.contains(tag) {
                return emit_inline_use(tag, attrs, children, ctx, out, &pad);
            }
            let is_component = ctx.components.contains(tag);
            let native = if is_component {
                if !ctx.used_components.contains(tag) {
                    ctx.used_components.push(tag.clone());
                }
                kebab(tag)
            } else {
                if !is_native_or_alias(tag) && !ctx.unknown_tags.iter().any(|(t, _)| t == tag) {
                    ctx.unknown_tags.push((tag.clone(), suggest_tag(tag)));
                }
                map_tag(tag)?.to_string()
            };
            let native = native.as_str();
            if attrs.iter().any(|a| a.name == "class") && attrs.iter().any(|a| a.name == "class:list") {
                return Err(format!(
                    "<{}> has both class and class:list — merge them into one class:list",
                    tag
                ));
            }
            out.push_str(&pad);
            out.push('<');
            out.push_str(native);
            for attr in attrs {
                if native == "navigator" && attr.name == "href" {
                    let renamed = Attr { name: "url".to_string(), value: clone_value(&attr.value) };
                    emit_attr(&renamed, ctx, out, is_component)?;
                } else {
                    emit_attr(attr, ctx, out, is_component)?;
                }
            }
            if children.is_empty() {
                out.push_str(" />\n");
            } else if children.len() == 1 && matches!(children[0], Node::Text(_) | Node::Expr(_)) {
                out.push('>');
                let mut inner = String::new();
                emit_node(&children[0], ctx, &mut inner, 0)?;
                out.push_str(inner.trim_end());
                out.push_str(&format!("</{}>\n", native));
            } else if children.iter().all(|c| matches!(c, Node::Text(_) | Node::Expr(_))) {
                out.push('>');
                let mut inner = String::new();
                for c in children {
                    emit_node(c, ctx, &mut inner, 0)?;
                }
                out.push_str(&inner.replace('\n', ""));
                out.push_str(&format!("</{}>\n", native));
            } else {
                out.push_str(">\n");
                emit_nodes(children, ctx, out, indent + 1)?;
                out.push_str(&pad);
                out.push_str(&format!("</{}>\n", native));
            }
        }
        Node::For { list, param, index, key, children } => {
            out.push_str(&pad);
            let key_attr = match key {
                Some(k) => format!(" wx:key=\"{}\"", k),
                None => String::new(),
            };
            // per-item hoisting: calls that reference the loop item become
            // computed `_c<i>` fields on a generated derived list
            let mut fields: Vec<String> = Vec::new();
            collect_item_calls(children, param, &ctx.call_re, &mut fields);
            if !fields.is_empty() && !ctx.loop_params.is_empty() {
                return Err(format!(
                    "M1009: call `{}` in a nested loop cannot be hoisted — the generated derived would capture `{}` outside its scope\n  help: precompute the values in frontmatter (e.g. a derived mapping the nested items)",
                    fields[0],
                    ctx.loop_params.join("`, `")
                ));
            }
            let (list_binding, pushed) = if fields.is_empty() {
                (ctx.bind(list), 0)
            } else {
                let name = format!("_hl{}", ctx.for_hoists.len());
                for (i, f) in fields.iter().enumerate() {
                    ctx.rewrites.push((f.clone(), format!("{}._c{}", param, i)));
                }
                ctx.for_hoists.push(ForHoist {
                    name: name.clone(),
                    list: list.clone(),
                    param: param.clone(),
                    key: key.clone(),
                    fields: fields.clone(),
                });
                (name, fields.len())
            };
            let index_attr = match index {
                Some(i) => format!(" wx:for-index=\"{}\"", i),
                None => String::new(),
            };
            out.push_str(&format!(
                "<block wx:for=\"{{{{{}}}}}\" wx:for-item=\"{}\"{}{}>\n",
                list_binding, param, index_attr, key_attr
            ));
            ctx.loop_params.push(param.clone());
            if let Some(i) = index {
                ctx.loop_params.push(i.clone());
            }
            let inner = emit_nodes(children, ctx, out, indent + 1);
            ctx.loop_params.pop();
            if index.is_some() {
                ctx.loop_params.pop();
            }
            inner?;
            for _ in 0..pushed {
                ctx.rewrites.pop();
            }
            out.push_str(&pad);
            out.push_str("</block>\n");
        }
        Node::If { cond, children, else_children } => {
            out.push_str(&pad);
            out.push_str(&format!("<block wx:if=\"{{{{{}}}}}\">\n", ctx.bind(cond)));
            emit_nodes(children, ctx, out, indent + 1)?;
            out.push_str(&pad);
            out.push_str("</block>\n");
            emit_else_chain(else_children, ctx, out, indent)?;
        }
    }
    Ok(())
}

/// Emits the else side of an `If`: a chained ternary (`else_children` holding
/// exactly one `Node::If`) becomes `<block wx:elif>`, recursing for its own
/// else side; anything else becomes a plain `<block wx:else>`.
fn emit_else_chain(else_children: &[Node], ctx: &mut Ctx, out: &mut String, indent: usize) -> Result<(), String> {
    let pad = "  ".repeat(indent);
    if let [Node::If { cond, children, else_children }] = else_children {
        out.push_str(&pad);
        out.push_str(&format!("<block wx:elif=\"{{{{{}}}}}\">\n", ctx.bind(cond)));
        emit_nodes(children, ctx, out, indent + 1)?;
        out.push_str(&pad);
        out.push_str("</block>\n");
        emit_else_chain(else_children, ctx, out, indent)?;
    } else if !else_children.is_empty() {
        out.push_str(&pad);
        out.push_str("<block wx:else>\n");
        emit_nodes(else_children, ctx, out, indent + 1)?;
        out.push_str(&pad);
        out.push_str("</block>\n");
    }
    Ok(())
}

/// `<Badge label={x} />` → `<template is="badge" data="{{ label: x }}" />`
fn emit_inline_use(
    tag: &str,
    attrs: &[Attr],
    children: &[Node],
    ctx: &mut Ctx,
    out: &mut String,
    pad: &str,
) -> Result<(), String> {
    if !children.is_empty() {
        return Err(format!("inlined component <{}> cannot take children (it has no slots)", tag));
    }
    if !ctx.used_inline.contains(&tag.to_string()) {
        ctx.used_inline.push(tag.to_string());
    }
    let mut pairs = Vec::new();
    for attr in attrs {
        if attr.name.strip_prefix("on").and_then(|r| r.chars().next()).is_some_and(|c| c.is_uppercase()) {
            return Err(format!(
                "inlined component <{}> cannot take callback prop '{}' — it declares none",
                tag, attr.name
            ));
        }
        match &attr.value {
            AttrValue::Static(s) => pairs.push(format!("{}: '{}'", attr.name, s)),
            AttrValue::Expr(e) => pairs.push(format!("{}: {}", attr.name, ctx.bind(e))),
            AttrValue::Bare => pairs.push(format!("{}: true", attr.name)),
        }
    }
    out.push_str(pad);
    if pairs.is_empty() {
        out.push_str(&format!("<template is=\"{}\" />\n", kebab(tag)));
    } else {
        out.push_str(&format!("<template is=\"{}\" data=\"{{{{ {} }}}}\" />\n", kebab(tag), pairs.join(", ")));
    }
    Ok(())
}

/// call-containing expressions that mention the loop item (skips nested loops)
fn collect_item_calls(nodes: &[Node], param: &str, call_re: &Regex, out: &mut Vec<String>) {
    let param_re = Regex::new(&format!(r"\b{}\b", regex::escape(param))).unwrap();
    fn consider(e: &str, call_re: &Regex, param_re: &Regex, out: &mut Vec<String>) {
        let t = e.trim();
        if call_re.is_match(t) && param_re.is_match(t) && !out.iter().any(|x| x == t) {
            out.push(t.to_string());
        }
    }
    for node in nodes {
        match node {
            Node::Expr(e) => consider(e, call_re, &param_re, out),
            Node::Element { attrs, children, .. } => {
                for a in attrs {
                    let is_event = a
                        .name
                        .strip_prefix("on")
                        .and_then(|r| r.chars().next())
                        .is_some_and(|c| c.is_uppercase());
                    if !is_event && a.name != "key" {
                        if let AttrValue::Expr(e) = &a.value {
                            consider(e, call_re, &param_re, out);
                        }
                    }
                }
                collect_item_calls(children, param, call_re, out);
            }
            Node::If { children, else_children, .. } => {
                collect_item_calls(children, param, call_re, out);
                collect_item_calls(else_children, param, call_re, out);
            }
            Node::For { .. } => {} // nested loops: not hoisted (document)
            Node::Text(_) => {}
        }
    }
}

fn clone_value(v: &AttrValue) -> AttrValue {
    match v {
        AttrValue::Static(s) => AttrValue::Static(s.clone()),
        AttrValue::Expr(e) => AttrValue::Expr(e.clone()),
        AttrValue::Bare => AttrValue::Bare,
    }
}

fn two_way_bind_prop(attr_name: &str) -> Option<&str> {
    let prop = attr_name.strip_suffix(":bind")?;
    is_ident(prop).then_some(prop)
}

fn model_companion(prop: &str) -> Option<&'static str> {
    match prop {
        "value" => Some("bindinput"),
        "checked" => Some("bindchange"),
        _ => None,
    }
}

fn emit_attr(attr: &Attr, ctx: &mut Ctx, out: &mut String, is_component: bool) -> Result<(), String> {
    if let Some(prop) = two_way_bind_prop(&attr.name) {
        let AttrValue::Expr(e) = &attr.value else {
            return Err(format!("{}:bind needs an expression: {}:bind={{text}}", prop, prop));
        };
        let name = e.trim().trim_end_matches(".value").trim_end_matches('.').to_string();
        if !is_ident(&name) {
            return Err(format!(
                "{}:bind needs a plain state name (got '{}') — {}:bind={{text}}",
                prop,
                e.trim(),
                prop
            ));
        }
        let companion = model_companion(prop).ok_or_else(|| {
            format!(
                "unsupported two-way binding '{}:bind' — supported: value:bind, checked:bind",
                prop
            )
        })?;
        // native model:<prop> renders without setData echo; the companion event handler
        // keeps the logic-side mirror + deriveds in sync through the normal batch
        out.push_str(&format!(" model:{}=\"{{{{{}}}}}\" {}=\"__vb_{}\"", prop, name, companion, name));
        if !ctx.vbinds.contains(&name) {
            ctx.vbinds.push(name);
        }
        return Ok(());
    }
    if let Some(event) = attr.name.strip_prefix("on") {
        if event.chars().next().is_some_and(|c| c.is_uppercase()) {
            return emit_event(attr, event, ctx, out, is_component);
        }
    }
    match &attr.value {
        AttrValue::Static(s) if attr.name == "class" => {
            let tokens: Vec<String> = s.split_whitespace().map(|t| crate::tailwind::sanitize(t)).collect();
            out.push_str(&format!(" class=\"{}\"", tokens.join(" ")));
        }
        AttrValue::Expr(e) if attr.name == "class" => {
            let e = crate::tailwind::sanitize_class_expr(e);
            let b = ctx.hoist_class_expr(&e)?;
            out.push_str(&format!(" class=\"{{{{{}}}}}\"", attr_escape(&b)));
        }
        AttrValue::Expr(e) if attr.name == "class:list" => {
            let value = class_list_value(e, ctx)?;
            out.push_str(&format!(" class=\"{}\"", value));
        }
        _ if attr.name == "class:list" => {
            return Err("class:list expects an array literal: class:list={[...]}".to_string());
        }
        AttrValue::Static(s) => out.push_str(&format!(" {}=\"{}\"", attr.name, s)),
        AttrValue::Expr(e) => {
            let b = ctx.hoist_or_bind(e)?;
            out.push_str(&format!(" {}=\"{{{{{}}}}}\"", attr.name, attr_escape(&b)));
        }
        AttrValue::Bare => out.push_str(&format!(" {}", attr.name)),
    }
    Ok(())
}

fn emit_event(attr: &Attr, event: &str, ctx: &mut Ctx, out: &mut String, is_component: bool) -> Result<(), String> {
    let (event, flavor) = match event.split_once(':') {
        Some((e, "catch")) => (e, "catch"),
        Some((e, "mut")) => (e, "mut-bind:"),
        Some((e, other)) => return Err(format!("unknown event modifier '{}' on on{}", other, e)),
        None => (event, "bind"),
    };
    let binding = if is_component {
        // component event: `onToggle` → `bind:toggle`
        format!("{}:{}", flavor.trim_end_matches(':'), crate::frontmatter::event_name(&format!("on{}", event)))
    } else {
        format!("{}{}", flavor, map_event(event))
    };
    let AttrValue::Expr(expr) = &attr.value else {
        return Err(format!("event on{} must be an expression", event));
    };
    let expr = expr.trim();

    if is_ident(expr) {
        if is_component {
            // child sends args via e.detail — wrap to unpack them
            let name = format!("_e{}", ctx.handlers.len());
            out.push_str(&format!(" {}=\"{}\"", binding, name));
            ctx.handlers.push(Handler {
                name,
                target: expr.to_string(),
                args: Vec::new(),
                from_detail: true,
            });
        } else {
            out.push_str(&format!(" {}=\"{}\"", binding, expr));
        }
        return Ok(());
    }

    // `() => target(arg, ...)`
    let Some(rest) = expr.strip_prefix("()") else {
        return Err(format!("unsupported event expression: {}", expr));
    };
    let rest = rest.trim_start().strip_prefix("=>").ok_or("expected arrow in event handler")?.trim();
    let (target, args) = parse_call(rest)?;
    let name = format!("_e{}", ctx.handlers.len());
    out.push_str(&format!(" {}=\"{}\"", binding, name));
    for (i, arg) in args.iter().enumerate() {
        out.push_str(&format!(" data-a{}=\"{{{{{}}}}}\"", i, ctx.bind(arg)));
    }
    ctx.handlers.push(Handler { name, target, args, from_detail: false });
    Ok(())
}

fn class_list_value(expr: &str, ctx: &mut Ctx) -> Result<String, String> {
    let t = expr.trim();
    let inner = t
        .strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .ok_or_else(|| format!("class:list expects an array literal, got `{}`", expr))?;
    let mut parts: Vec<String> = Vec::new();
    for item in split_top_level_commas(inner) {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let quoted = (item.starts_with('\'') && item.ends_with('\'') && item.len() >= 2)
            || (item.starts_with('"') && item.ends_with('"') && item.len() >= 2);
        if quoted {
            let content = &item[1..item.len() - 1];
            let tokens: Vec<String> =
                content.split_whitespace().map(|c| crate::tailwind::sanitize(c)).collect();
            if !tokens.is_empty() {
                parts.push(tokens.join(" "));
            }
        } else if item.starts_with('{') && item.ends_with('}') {
            let body = &item[1..item.len() - 1];
            for entry in split_top_level_commas(body) {
                let entry = entry.trim();
                if entry.is_empty() {
                    continue;
                }
                let Some(colon) = top_level_colon(entry) else {
                    return Err(format!(
                        "class:list object entries need `class: condition` form, got `{}`",
                        entry
                    ));
                };
                let key = entry[..colon].trim().trim_matches('\'').trim_matches('"');
                let classes: Vec<String> =
                    key.split_whitespace().map(|c| crate::tailwind::sanitize(c)).collect();
                let cond = entry[colon + 1..].trim();
                let b = ctx.hoist_or_bind(cond)?;
                parts.push(format!("{{{{{} ? '{}' : ''}}}}", attr_escape(&b), classes.join(" ")));
            }
        } else {
            let e = crate::tailwind::sanitize_class_expr(item);
            let b = ctx.hoist_class_expr(&e)?;
            parts.push(format!("{{{{({}) || ''}}}}", attr_escape(&b)));
        }
    }
    Ok(parts.join(" "))
}

pub(crate) fn top_level_colon(s: &str) -> Option<usize> {
    let mut depth = 0i32;
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
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ':' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

fn parse_call(s: &str) -> Result<(String, Vec<String>), String> {
    let open = s.find('(').ok_or_else(|| format!("expected call in event handler: {}", s))?;
    let target = s[..open].trim().to_string();
    let inner = s[open + 1..].strip_suffix(')').ok_or("expected ')' in event handler")?;
    let args = split_top_level_commas(inner)
        .into_iter()
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect();
    Ok((target, args))
}

pub(crate) fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut cur = String::new();
    for c in s.chars() {
        if let Some(q) = quote {
            cur.push(c);
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' | '`' => {
                quote = Some(c);
                cur.push(c);
            }
            '(' | '[' | '{' => {
                depth += 1;
                cur.push(c);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        parts.push(cur);
    }
    parts
}

impl Ctx {
    fn bind(&self, expr: &str) -> String {
        let t = expr.trim();
        if let Some((_, r)) = self.rewrites.iter().find(|(e, _)| e == t) {
            return r.clone();
        }
        self.strip_value.replace_all(expr, "$1").to_string()
    }

    /// Bindings WXML cannot evaluate (calls, template literals, optional
    /// chaining) are hoisted into a generated page-scope derived (`_h<i>`).
    /// Loop-item calls are handled by the For pre-pass (rewrites) first.
    fn hoist_or_bind(&mut self, expr: &str) -> Result<String, String> {
        let t = expr.trim();
        if let Some((_, r)) = self.rewrites.iter().find(|(e, _)| e == t) {
            return Ok(r.clone());
        }
        let wxml_hostile = self.call_re.is_match(t) || t.contains('`') || t.contains("?.");
        if !wxml_hostile {
            return Ok(self.bind(expr));
        }
        if let Some(p) = self.loop_params.iter().find(|p| word_in(t, p)) {
            return Err(format!(
                "M1009: `{}` references loop item `{}` but cannot run in WXML\n  help: calls on the item are hoisted automatically; move template literals or optional chaining into a computed field or a derived",
                t, p
            ));
        }
        let name = format!("_h{}", self.hoisted.len());
        self.hoisted.push(t.to_string());
        Ok(name)
    }

    fn hoist_class_expr(&mut self, expr: &str) -> Result<String, String> {
        let before = self.hoisted.len();
        let b = self.hoist_or_bind(expr)?;
        if self.hoisted.len() > before {
            self.class_hoists.push(before);
        }
        Ok(b)
    }
}

pub(crate) fn word_in(haystack: &str, word: &str) -> bool {
    let ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    for (i, _) in haystack.match_indices(word) {
        let before_ok = haystack[..i].chars().next_back().map(|c| !ident(c)).unwrap_or(true);
        let after_ok =
            haystack[i + word.len()..].chars().next().map(|c| !ident(c)).unwrap_or(true);
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

fn attr_escape(s: &str) -> String {
    s.replace('"', "&quot;")
}

fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

pub(crate) fn map_tag(tag: &str) -> Result<&str, String> {
    Ok(match tag {
        "div" | "section" | "header" | "footer" | "main" | "article" | "ul" | "ol" | "li"
        | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "nav" | "aside" => "view",
        "span" => "text",
        "img" => "image",
        "a" => "navigator",
        t => t,
    })
}

/// Web-alias source tags handled by [`map_tag`] — kept separate from
/// [`NATIVE_TAGS`] so M1019 can check both without re-deriving one from the other.
const WEB_ALIAS_TAGS: &[&str] = &[
    "div", "section", "header", "footer", "main", "article", "ul", "ol", "li", "p", "h1", "h2",
    "h3", "h4", "h5", "h6", "nav", "aside", "span", "img", "a",
];

/// WeChat Mini Program native component tags — used by M1019 to flag tags
/// that are neither native, a web alias, nor a registered component.
pub(crate) const NATIVE_TAGS: &[&str] = &[
    "view",
    "scroll-view",
    "swiper",
    "swiper-item",
    "movable-area",
    "movable-view",
    "cover-view",
    "cover-image",
    "icon",
    "text",
    "rich-text",
    "progress",
    "button",
    "checkbox",
    "checkbox-group",
    "form",
    "input",
    "label",
    "picker",
    "picker-view",
    "picker-view-column",
    "radio",
    "radio-group",
    "slider",
    "switch",
    "textarea",
    "editor",
    "navigator",
    "functional-page-navigator",
    "audio",
    "camera",
    "image",
    "live-player",
    "live-pusher",
    "video",
    "map",
    "canvas",
    "web-view",
    "ad",
    "ad-custom",
    "official-account",
    "open-data",
    "navigation-bar",
    "page-meta",
    "page-container",
    "share-element",
    "keyboard-accessory",
    "root-portal",
    "match-media",
    "channel-live",
    "channel-video",
    "voip-room",
    "store-home",
    "store-product",
    "block",
    "slot",
    "template",
    "import",
    "include",
    "wxs",
];

fn is_native_or_alias(tag: &str) -> bool {
    NATIVE_TAGS.contains(&tag) || WEB_ALIAS_TAGS.contains(&tag)
}

/// Nearest NATIVE_TAGS/web-alias name within edit distance 2, if any.
fn suggest_tag(tag: &str) -> Option<String> {
    NATIVE_TAGS
        .iter()
        .chain(WEB_ALIAS_TAGS.iter())
        .map(|&candidate| (candidate, levenshtein(tag, candidate)))
        .filter(|(_, dist)| *dist <= 2)
        .min_by_key(|(_, dist)| *dist)
        .map(|(candidate, _)| candidate.to_string())
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (curr[j - 1] + 1).min(prev[j] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

fn map_event(event: &str) -> String {
    let lower = event.to_lowercase();
    match lower.as_str() {
        "click" => "tap".to_string(),
        _ => lower,
    }
}
