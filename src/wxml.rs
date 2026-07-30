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
    pub vbinds: Vec<String>,
    pub hoisted: Vec<String>,
    pub for_hoists: Vec<ForHoist>,
}

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
    for_hoists: Vec<ForHoist>,
    rewrites: Vec<(String, String)>,
    call_re: Regex,
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
        for_hoists: Vec::new(),
        rewrites: Vec::new(),
        call_re: Regex::new(r"[A-Za-z0-9_\]]\s*\(").map_err(|e| e.to_string())?,
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
        for_hoists: ctx.for_hoists,
    })
}

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
            let b = ctx.hoist_or_bind(e);
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
                map_tag(tag)?.to_string()
            };
            let native = native.as_str();
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
        Node::For { list, param, key, children } => {
            out.push_str(&pad);
            let key_attr = match key {
                Some(k) => format!(" wx:key=\"{}\"", k),
                None => String::new(),
            };
            let mut fields: Vec<String> = Vec::new();
            collect_item_calls(children, param, &ctx.call_re, &mut fields);
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
            out.push_str(&format!(
                "<block wx:for=\"{{{{{}}}}}\" wx:for-item=\"{}\"{}>\n",
                list_binding, param, key_attr
            ));
            emit_nodes(children, ctx, out, indent + 1)?;
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
            if !else_children.is_empty() {
                out.push_str(&pad);
                out.push_str("<block wx:else>\n");
                emit_nodes(else_children, ctx, out, indent + 1)?;
                out.push_str(&pad);
                out.push_str("</block>\n");
            }
        }
    }
    Ok(())
}

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
            Node::For { .. } => {}
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

fn emit_attr(attr: &Attr, ctx: &mut Ctx, out: &mut String, is_component: bool) -> Result<(), String> {
    if attr.name == "value:bind" {
        let AttrValue::Expr(e) = &attr.value else {
            return Err("value:bind needs an expression: value:bind={text}".to_string());
        };
        let name = e.trim().trim_end_matches(".value").trim_end_matches('.').to_string();
        if !is_ident(&name) {
            return Err(format!(
                "value:bind needs a plain state name (got '{}') — value:bind={{text}}",
                e.trim()
            ));
        }
        out.push_str(&format!(" model:value=\"{{{{{}}}}}\" bindinput=\"__vb_{}\"", name, name));
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
            let tokens: Vec<String> = s.split_whitespace().map(crate::tailwind::sanitize).collect();
            out.push_str(&format!(" class=\"{}\"", tokens.join(" ")));
        }
        AttrValue::Expr(e) if attr.name == "class" => {
            let e = crate::tailwind::sanitize_class_expr(e);
            let b = ctx.hoist_or_bind(&e);
            out.push_str(&format!(" class=\"{{{{{}}}}}\"", b));
        }
        AttrValue::Static(s) => out.push_str(&format!(" {}=\"{}\"", attr.name, s)),
        AttrValue::Expr(e) => {
            let b = ctx.hoist_or_bind(e);
            out.push_str(&format!(" {}=\"{{{{{}}}}}\"", attr.name, b));
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

fn split_top_level_commas(s: &str) -> Vec<String> {
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

    /// Bindings that contain calls can't run in WXML — hoist reactive ones into
    fn hoist_or_bind(&mut self, expr: &str) -> String {
        let t = expr.trim();
        if let Some((_, r)) = self.rewrites.iter().find(|(e, _)| e == t) {
            return r.clone();
        }
        if self.call_re.is_match(t) && t.contains(".value") {
            let name = format!("_h{}", self.hoisted.len());
            self.hoisted.push(t.to_string());
            return name;
        }
        self.bind(expr)
    }
}

fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn map_tag(tag: &str) -> Result<&str, String> {
    Ok(match tag {
        "div" | "section" | "header" | "footer" | "main" | "article" | "ul" | "ol" | "li"
        | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "nav" | "aside" => "view",
        "span" => "text",
        "img" => "image",
        "a" => "navigator",
        t => t,
    })
}

fn map_event(event: &str) -> String {
    let lower = event.to_lowercase();
    match lower.as_str() {
        "click" => "tap".to_string(),
        _ => lower,
    }
}
