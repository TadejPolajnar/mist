#[derive(Debug)]
pub enum Node {
    Element { tag: String, attrs: Vec<Attr>, children: Vec<Node> },
    Text(String),
    Expr(String),
    For { list: String, param: String, index: Option<String>, key: Option<String>, children: Vec<Node> },
    If { cond: String, children: Vec<Node>, else_children: Vec<Node> },
}

#[derive(Debug)]
pub struct Attr {
    pub name: String,
    pub value: AttrValue,
}

#[derive(Debug)]
pub enum AttrValue {
    Static(String),
    Expr(String),
    Bare,
}

/// (list expression, wx:key) pairs for every loop in the tree
pub fn for_lists(nodes: &[Node]) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    collect_for_lists(nodes, &mut out);
    out
}

fn collect_for_lists(nodes: &[Node], out: &mut Vec<(String, Option<String>)>) {
    for node in nodes {
        match node {
            Node::For { list, key, children, .. } => {
                out.push((list.clone(), key.clone()));
                collect_for_lists(children, out);
            }
            Node::Element { children, .. } => collect_for_lists(children, out),
            Node::If { children, else_children, .. } => {
                collect_for_lists(children, out);
                collect_for_lists(else_children, out);
            }
            _ => {}
        }
    }
}

pub fn bare_reactive_refs(nodes: &[Node], names: &[String]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut params = Vec::new();
    collect_bare_refs(nodes, names, &mut params, &mut out);
    out
}

fn collect_bare_refs(
    nodes: &[Node],
    names: &[String],
    params: &mut Vec<String>,
    out: &mut Vec<(String, String)>,
) {
    for node in nodes {
        match node {
            Node::Expr(e) => scan_bare(e, names, params, out),
            Node::Element { attrs, children, .. } => {
                for a in attrs {
                    if a.name.ends_with(":bind") {
                        continue;
                    }
                    if let AttrValue::Expr(e) = &a.value {
                        scan_bare(e, names, params, out);
                    }
                }
                collect_bare_refs(children, names, params, out);
            }
            Node::For { list, param, children, .. } => {
                scan_bare(list, names, params, out);
                params.push(param.clone());
                collect_bare_refs(children, names, params, out);
                params.pop();
            }
            Node::If { cond, children, else_children } => {
                scan_bare(cond, names, params, out);
                collect_bare_refs(children, names, params, out);
                collect_bare_refs(else_children, names, params, out);
            }
            Node::Text(_) => {}
        }
    }
}

fn ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

fn string_mask(e: &str) -> Vec<bool> {
    let bytes = e.as_bytes();
    let mut mask = vec![false; bytes.len()];
    let mut stack: Vec<u8> = Vec::new();
    let mut brace_depths: Vec<u32> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match stack.last().copied() {
            Some(q) if q != b'{' => {
                mask[i] = true;
                if b == b'\\' {
                    if i + 1 < bytes.len() {
                        mask[i + 1] = true;
                    }
                    i += 1;
                } else if b == q {
                    stack.pop();
                } else if q == b'`' && b == b'$' && bytes.get(i + 1) == Some(&b'{') {
                    mask[i + 1] = true;
                    stack.push(b'{');
                    brace_depths.push(0);
                    i += 1;
                }
            }
            Some(_) => match b {
                b'\'' | b'"' | b'`' => {
                    mask[i] = true;
                    stack.push(b);
                }
                b'{' => {
                    if let Some(d) = brace_depths.last_mut() {
                        *d += 1;
                    }
                }
                b'}' => match brace_depths.last_mut() {
                    Some(0) | None => {
                        mask[i] = true;
                        stack.pop();
                        brace_depths.pop();
                    }
                    Some(d) => *d -= 1,
                },
                _ => {}
            },
            None => {
                if b == b'\'' || b == b'"' || b == b'`' {
                    mask[i] = true;
                    stack.push(b);
                }
            }
        }
        i += 1;
    }
    mask
}

fn scan_bare(e: &str, names: &[String], params: &[String], out: &mut Vec<(String, String)>) {
    let mask = string_mask(e);
    let bytes = e.as_bytes();
    for name in names {
        if params.iter().any(|p| p == name) {
            continue;
        }
        for (i, _) in e.match_indices(name.as_str()) {
            if mask.get(i).copied().unwrap_or(false) {
                continue;
            }
            if i > 0 && bytes[i - 1] == b'.' {
                continue;
            }
            let before_ok = i == 0 || !ident_byte(bytes[i - 1]);
            let end = i + name.len();
            let after_ok = end >= bytes.len() || !ident_byte(bytes[end]);
            if !before_ok || !after_ok {
                continue;
            }
            if e[end..].starts_with(".value") {
                continue;
            }
            if !out.iter().any(|(n, x)| n == name && x == e) {
                out.push((name.clone(), e.to_string()));
            }
        }
    }
}

/// Names read as `name.value` in an expression position (`Node::Expr`, `AttrValue::Expr`,
/// `For.list`, `If.cond`/`else_children`) — the real template read surface, as opposed to
/// static attr text where `name.value` is just a string and not a read.
pub fn collect_expr_reads(nodes: &[Node], names: &[String]) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    collect_expr_reads_inner(nodes, names, &mut out);
    out
}

fn collect_expr_reads_inner(nodes: &[Node], names: &[String], out: &mut std::collections::HashSet<String>) {
    for node in nodes {
        match node {
            Node::Expr(e) => scan_value_reads(e, names, out),
            Node::Element { attrs, children, .. } => {
                for a in attrs {
                    if let AttrValue::Expr(e) = &a.value {
                        scan_value_reads(e, names, out);
                    }
                }
                collect_expr_reads_inner(children, names, out);
            }
            Node::For { list, children, .. } => {
                scan_value_reads(list, names, out);
                collect_expr_reads_inner(children, names, out);
            }
            Node::If { cond, children, else_children } => {
                scan_value_reads(cond, names, out);
                collect_expr_reads_inner(children, names, out);
                collect_expr_reads_inner(else_children, names, out);
            }
            Node::Text(_) => {}
        }
    }
}

/// State names targeted by a `<prop>:bind={name}` two-way binding attr, e.g.
/// `value:bind={on}` or `checked:bind={on}` — mirrors wxml::emit_attr's own parsing of
/// the same attrs so a `:bind`-only state is known bound before the mutation rewriter runs.
pub fn collect_bind_targets(nodes: &[Node]) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    collect_bind_targets_inner(nodes, &mut out);
    out
}

fn collect_bind_targets_inner(nodes: &[Node], out: &mut std::collections::HashSet<String>) {
    for node in nodes {
        match node {
            Node::Element { attrs, children, .. } => {
                for a in attrs {
                    let Some(prop) = a.name.strip_suffix(":bind") else { continue };
                    if !is_ident(prop) {
                        continue;
                    }
                    let AttrValue::Expr(e) = &a.value else { continue };
                    let name = e.trim().trim_end_matches(".value").trim_end_matches('.').to_string();
                    if is_ident(&name) {
                        out.insert(name);
                    }
                }
                collect_bind_targets_inner(children, out);
            }
            Node::For { children, .. } => collect_bind_targets_inner(children, out),
            Node::If { children, else_children, .. } => {
                collect_bind_targets_inner(children, out);
                collect_bind_targets_inner(else_children, out);
            }
            Node::Text(_) | Node::Expr(_) => {}
        }
    }
}

fn scan_value_reads(e: &str, names: &[String], out: &mut std::collections::HashSet<String>) {
    let mask = string_mask(e);
    let bytes = e.as_bytes();
    for name in names {
        if out.contains(name) {
            continue;
        }
        for (i, _) in e.match_indices(name.as_str()) {
            if mask.get(i).copied().unwrap_or(false) {
                continue;
            }
            let before_ok = i == 0 || !ident_byte(bytes[i - 1]);
            let end = i + name.len();
            if !before_ok || !e[end..].starts_with(".value") {
                continue;
            }
            let after_value = end + ".value".len();
            let after_ok = after_value >= bytes.len() || !ident_byte(bytes[after_value]);
            if !after_ok {
                continue;
            }
            out.insert(name.clone());
            break;
        }
    }
}

pub fn has_slot(nodes: &[Node]) -> bool {
    any_element(nodes, &mut |tag, _| tag == "slot")
}

pub fn has_named_slot(nodes: &[Node]) -> bool {
    any_element(nodes, &mut |tag, attrs| {
        tag == "slot" && attrs.iter().any(|a| a.name == "name")
    })
}

pub fn has_events(nodes: &[Node]) -> bool {
    any_element(nodes, &mut |_, attrs| {
        attrs.iter().any(|a| {
            a.name
                .strip_prefix("on")
                .and_then(|r| r.chars().next())
                .is_some_and(|c| c.is_uppercase())
        })
    })
}

/// SPEC §4.1: an element mapping to native `text` may only contain text/`text`
/// children — box styling on a nested `view` etc. silently no-ops. Returns
/// `(parent_tag, child_tag)` for each violating child, in document order.
pub fn text_box_child_violations(nodes: &[Node]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    collect_text_box_violations(nodes, &mut out);
    out
}

fn collect_text_box_violations(nodes: &[Node], out: &mut Vec<(String, String)>) {
    for node in nodes {
        match node {
            Node::Element { tag, children, .. } => {
                if crate::wxml::map_tag(tag).ok() == Some("text") {
                    check_text_children(tag, children, out);
                }
                collect_text_box_violations(children, out);
            }
            Node::For { children, .. } => collect_text_box_violations(children, out),
            Node::If { children, else_children, .. } => {
                collect_text_box_violations(children, out);
                collect_text_box_violations(else_children, out);
            }
            _ => {}
        }
    }
}

/// Checks the direct children of a `text`-mapped element, recursing through
/// `If`/`For` wrappers at that position (a conditional `<div>` inside a span
/// is still a violation) without crossing another element boundary.
fn check_text_children(parent_tag: &str, children: &[Node], out: &mut Vec<(String, String)>) {
    for node in children {
        match node {
            Node::Text(_) | Node::Expr(_) => {}
            Node::Element { tag, .. } => {
                if crate::wxml::map_tag(tag).ok() != Some("text") {
                    out.push((parent_tag.to_string(), tag.clone()));
                }
            }
            Node::For { children, .. } => check_text_children(parent_tag, children, out),
            Node::If { children, else_children, .. } => {
                check_text_children(parent_tag, children, out);
                check_text_children(parent_tag, else_children, out);
            }
        }
    }
}

fn any_element(nodes: &[Node], pred: &mut dyn FnMut(&str, &[Attr]) -> bool) -> bool {
    for node in nodes {
        match node {
            Node::Element { tag, attrs, children } => {
                if pred(tag, attrs) || any_element(children, pred) {
                    return true;
                }
            }
            Node::For { children, .. } => {
                if any_element(children, pred) {
                    return true;
                }
            }
            Node::If { children, else_children, .. } => {
                if any_element(children, pred) || any_element(else_children, pred) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

pub fn parse(src: &str) -> Result<Vec<Node>, String> {
    parse_at(src, 1)
}

/// `line_offset`: 1-based line of the template's first line within the `.mist` file.
pub fn parse_at(src: &str, line_offset: usize) -> Result<Vec<Node>, String> {
    let mut p = Parser { chars: src.chars().collect(), pos: 0, line_offset };
    let nodes = p.parse_nodes(None)?;
    Ok(nodes)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    line_offset: usize,
}

impl Parser {
    fn line(&self) -> usize {
        self.chars[..self.pos.min(self.chars.len())].iter().filter(|c| **c == '\n').count()
            + self.line_offset
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn starts_with(&self, s: &str) -> bool {
        s.chars().enumerate().all(|(i, c)| self.chars.get(self.pos + i) == Some(&c))
    }

    fn parse_nodes(&mut self, closing_tag: Option<&str>) -> Result<Vec<Node>, String> {
        let mut nodes = Vec::new();
        let mut text = String::new();
        loop {
            match self.peek() {
                None => {
                    flush_text(&mut text, &mut nodes);
                    if closing_tag.is_some() {
                        return Err(format!("M1010 at line {}: unexpected EOF, expected </{}>", self.line(), closing_tag.unwrap()));
                    }
                    return Ok(nodes);
                }
                Some('<') if self.starts_with("</") => {
                    flush_text(&mut text, &mut nodes);
                    let tag = closing_tag.ok_or("unexpected closing tag")?;
                    let expect = format!("</{}>", tag);
                    if !self.starts_with(&expect) {
                        return Err(format!("M1010 at line {}: mismatched closing tag, expected {}", self.line(), expect));
                    }
                    self.pos += expect.len();
                    return Ok(nodes);
                }
                Some('<') => {
                    flush_text(&mut text, &mut nodes);
                    nodes.push(self.parse_element()?);
                }
                Some('{') => {
                    let line = self.line();
                    let start = self.pos;
                    let inner = self.read_braced()?;
                    match classify_expr(&inner, line)? {
                        Node::Text(_) => {
                            text.extend(&self.chars[start..self.pos]);
                        }
                        node => {
                            flush_text(&mut text, &mut nodes);
                            nodes.push(node);
                        }
                    }
                }
                Some(c) => {
                    text.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    fn parse_element(&mut self) -> Result<Node, String> {
        self.pos += 1; // '<'
        let tag = self.read_ident_like();
        if tag.is_empty() {
            return Err(format!("M1010 at line {}: expected tag name", self.line()));
        }
        let attrs = self.parse_attrs()?;
        if self.starts_with("/>") {
            self.pos += 2;
            return Ok(Node::Element { tag, attrs, children: Vec::new() });
        }
        if self.peek() == Some('>') {
            self.pos += 1;
            let children = self.parse_nodes(Some(&tag))?;
            return Ok(Node::Element { tag, attrs, children });
        }
        Err(format!("malformed tag <{}>", tag))
    }

    fn parse_attrs(&mut self) -> Result<Vec<Attr>, String> {
        let mut attrs = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                Some('>') | Some('/') | None => return Ok(attrs),
                _ => {}
            }
            let name = self.read_attr_name();
            if name.is_empty() {
                return Err(format!("M1010 at line {}: expected attribute", self.line()));
            }
            if self.peek() != Some('=') {
                attrs.push(Attr { name, value: AttrValue::Bare });
                continue;
            }
            self.pos += 1; // '='
            let value = match self.peek() {
                Some('"') => {
                    self.pos += 1;
                    let mut s = String::new();
                    while let Some(c) = self.peek() {
                        if c == '"' {
                            break;
                        }
                        s.push(c);
                        self.pos += 1;
                    }
                    self.pos += 1;
                    AttrValue::Static(s)
                }
                Some('{') => AttrValue::Expr(self.read_braced()?),
                _ => return Err(format!("malformed attribute value for '{}'", name)),
            };
            attrs.push(Attr { name, value });
        }
    }

    fn read_braced(&mut self) -> Result<String, String> {
        // self.pos is at '{'; returns inner content with balanced braces/quotes handled
        let mut depth = 0usize;
        let mut out = String::new();
        let mut quote: Option<char> = None;
        loop {
            let c = self.peek().ok_or("unbalanced '{'")?;
            self.pos += 1;
            if let Some(q) = quote {
                out.push(c);
                if c == '\\' {
                    if let Some(next) = self.peek() {
                        out.push(next);
                        self.pos += 1;
                    }
                } else if c == q {
                    quote = None;
                }
                continue;
            }
            match c {
                '\'' | '"' | '`' => {
                    out.push(c);
                    quote = Some(c);
                }
                '{' => {
                    depth += 1;
                    if depth > 1 {
                        out.push(c);
                    }
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        // strip the opening brace we skipped
                        return Ok(out.trim().to_string());
                    }
                    out.push(c);
                }
                _ => out.push(c),
            }
        }
    }

    fn read_ident_like(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                s.push(c);
                self.pos += 1;
            } else {
                break;
            }
        }
        s
    }

    fn read_attr_name(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == ':' {
                s.push(c);
                self.pos += 1;
            } else {
                break;
            }
        }
        s
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }
}

fn flush_text(text: &mut String, nodes: &mut Vec<Node>) {
    if text.trim().is_empty() {
        text.clear();
        return;
    }
    let mut t = text.as_str();
    // drop indentation-only edges (whitespace runs containing a newline)
    if let Some(i) = t.find(|c: char| !c.is_whitespace()) {
        if t[..i].contains('\n') {
            t = &t[i..];
        }
    }
    if let Some(i) = t.rfind(|c: char| !c.is_whitespace()) {
        let end = i + t[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        if t[end..].contains('\n') {
            t = &t[..end];
        }
    }
    nodes.push(Node::Text(t.to_string()));
    text.clear();
}

fn classify_expr(inner: &str, line: usize) -> Result<Node, String> {
    if let Some(node) = try_parse_map(inner, line)? {
        return Ok(node);
    }
    if let Some(node) = try_parse_ternary(inner, line)? {
        return Ok(node);
    }
    if let Some(node) = try_parse_logical_and(inner)? {
        return Ok(node);
    }
    if !parses_as_expression(inner) {
        return Ok(Node::Text(format!("{{{}}}", inner)));
    }
    Ok(Node::Expr(inner.to_string()))
}

fn parses_as_expression(inner: &str) -> bool {
    use oxc_parser::Parser;
    use oxc_span::SourceType;
    let allocator = oxc_allocator::Allocator::default();
    let wrapped = format!("({})", inner);
    let ret = Parser::new(&allocator, &wrapped, SourceType::default().with_typescript(true)).parse();
    ret.errors.is_empty() && ret.program.body.len() == 1
}

fn strip_parens(s: &str) -> &str {
    let t = s.trim();
    if t.starts_with('(') && t.ends_with(')') {
        t[1..t.len() - 1].trim()
    } else {
        t
    }
}

/// `cond ? <a/> : <b/>` (both branches optionally parenthesized) → `Node::If`
/// with `else_children`. Chained ternaries in the else branch recurse into
/// `else_children: [Node::If ..]`, which the emitter turns into `wx:elif`.
fn try_parse_ternary(inner: &str, line: usize) -> Result<Option<Node>, String> {
    let Some((q_idx, c_idx)) = find_top_level_ternary(inner) else {
        return Ok(None);
    };
    let then_branch = strip_parens(&inner[q_idx + 1..c_idx]);
    let else_branch = strip_parens(&inner[c_idx + 1..]);
    let then_is_jsx = then_branch.starts_with('<');
    let chained_else = try_parse_ternary(else_branch, line)?;
    let else_is_jsx = else_branch.starts_with('<') || chained_else.is_some();
    if !then_is_jsx && !else_is_jsx {
        return Ok(None);
    }
    if then_is_jsx != else_is_jsx {
        return Err(format!(
            "M1010 at line {}: both branches of a JSX ternary must be elements — wrap the text side in <span> or use && for a single branch",
            line
        ));
    }
    let cond = inner[..q_idx].trim().to_string();
    let children = parse(then_branch)?;
    let else_children = if let Some(else_if) = chained_else {
        vec![else_if]
    } else {
        parse(else_branch)?
    };
    Ok(Some(Node::If { cond, children, else_children }))
}

/// `list.map(param => (<jsx/>))` or `list.map(param => <jsx/>)`
fn try_parse_map(inner: &str, line: usize) -> Result<Option<Node>, String> {
    let Some(map_idx) = find_top_level(inner, ".map(") else {
        return Ok(None);
    };
    let list = inner[..map_idx].trim().to_string();
    let after = &inner[map_idx + 5..];
    let Some(arrow_idx) = after.find("=>") else {
        return Ok(None);
    };
    let params_raw = after[..arrow_idx].trim().trim_matches(|c| c == '(' || c == ')').trim().to_string();
    let (param, index) = parse_map_params(&params_raw, line)?;
    let mut body = after[arrow_idx + 2..].trim();
    // strip trailing `)` closing the .map( call
    body = body.strip_suffix(')').ok_or("expected ')' closing .map()")?.trim();
    if body.starts_with('(') && body.ends_with(')') {
        body = body[1..body.len() - 1].trim();
    }
    let mut children = if body.starts_with('<') {
        parse(body)?
    } else if let Some(if_node) = try_parse_ternary(body, line)? {
        vec![if_node]
    } else {
        return Ok(None);
    };
    let key = extract_key(&mut children, &param, index.as_deref())?;
    Ok(Some(Node::For { list, param, index, key, children }))
}

fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with(|c: char| c.is_ascii_digit())
        && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// `(item)` or `(item, index)` — anything else (destructuring, extra args) is M1010.
fn parse_map_params(raw: &str, line: usize) -> Result<(String, Option<String>), String> {
    let parts: Vec<&str> = raw.split(',').map(|p| p.trim()).collect();
    if parts.len() > 2 || parts.iter().any(|p| !is_ident(p)) {
        return Err(format!(
            "M1010 at line {}: unsupported .map() callback parameters `({})` — only `(item)` or `(item, index)` are supported",
            line, raw
        ));
    }
    let param = parts[0].to_string();
    let index = parts.get(1).map(|s| s.to_string());
    Ok((param, index))
}

/// `cond && <jsx/>`
fn try_parse_logical_and(inner: &str) -> Result<Option<Node>, String> {
    let Some(idx) = find_top_level(inner, "&&") else {
        return Ok(None);
    };
    let mut rest = inner[idx + 2..].trim();
    // allow the common JSX style: cond && (<jsx/>)
    if rest.starts_with('(') && rest.ends_with(')') {
        rest = rest[1..rest.len() - 1].trim();
    }
    if !rest.starts_with('<') {
        return Ok(None);
    }
    let cond = inner[..idx].trim().to_string();
    let children = parse(rest)?;
    Ok(Some(Node::If { cond, children, else_children: Vec::new() }))
}

/// `wx:key` accepts only a direct property of the loop item, the item itself
/// (`*this`), or `index` — anything computed is an M1003 error.
///
/// Walks the whole loop body (into `Element` children and both branches of
/// `If`, but not into a nested `For`, which owns its own key) so a key
/// wrapped in a conditional or on a non-first sibling is still found.
fn extract_key(children: &mut [Node], param: &str, index: Option<&str>) -> Result<Option<String>, String> {
    let mut found = Vec::new();
    collect_keys(children, param, index, &mut found)?;
    match found.len() {
        0 => Ok(None),
        1 => Ok(found.into_iter().next().unwrap()),
        _ => Err("M1003: multiple key= attributes in one loop body — keep exactly one".to_string()),
    }
}

fn collect_keys(
    children: &mut [Node],
    param: &str,
    index: Option<&str>,
    found: &mut Vec<Option<String>>,
) -> Result<(), String> {
    for node in children.iter_mut() {
        match node {
            Node::Element { attrs, children, .. } => {
                if let Some(i) = attrs.iter().position(|a| a.name == "key") {
                    let attr = attrs.remove(i);
                    found.push(resolve_key(attr, param, index)?);
                }
                collect_keys(children, param, index, found)?;
            }
            Node::If { children, else_children, .. } => {
                collect_keys(children, param, index, found)?;
                collect_keys(else_children, param, index, found)?;
            }
            Node::For { .. } | Node::Text(_) | Node::Expr(_) => {}
        }
    }
    Ok(())
}

fn resolve_key(attr: Attr, param: &str, index: Option<&str>) -> Result<Option<String>, String> {
    if let AttrValue::Expr(e) = attr.value {
        let e = e.trim().to_string();
        if e == "index" || Some(e.as_str()) == index {
            return Ok(None);
        }
        if e == param {
            return Ok(Some("*this".to_string()));
        }
        if let Some(prop) = e.strip_prefix(&format!("{}.", param)) {
            if !prop.is_empty() && prop.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
                return Ok(Some(prop.to_string()));
            }
        }
        return Err(format!(
            "M1003: key={{{}}} — wx:key needs a direct property of `{}` (e.g. key={{{}.id}}), `{}` itself, or `index`",
            e, param, param, param
        ));
    }
    Err(format!("M1003: key must be an expression like key={{{}.id}}", param))
}

/// Finds the first top-level `?` (skipping `?.` and `??`) and its matching
/// top-level `:`, tracking nested ternaries via a pending-`?` counter since
/// nested ternaries and object literals also contain `:`.
///
/// Once the then-branch is known (right after the `?`) to start directly
/// with `<` (unparenthesized JSX), the scan jumps past that element's root
/// tag via `skip_jsx_element` before resuming — otherwise a `:` inside a
/// colon-bearing attribute name (`value:bind`, `onTap:catch`, …) would be
/// mistaken for the branch separator.
fn find_top_level_ternary(s: &str) -> Option<(usize, usize)> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut i = 0;
    let mut q_idx = None;
    let mut pending = 0i32;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'\'' | b'"' | b'`' => {
                quote = Some(c);
                i += 1;
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            if c == b'?' {
                let next = bytes.get(i + 1).copied();
                if next == Some(b'.') || next == Some(b'?') {
                    i += 2;
                    continue;
                }
                if q_idx.is_none() {
                    q_idx = Some(i);
                    let rest = s[i + 1..].trim_start();
                    if rest.starts_with('<') {
                        let rest_start = s.len() - rest.len();
                        if let Some(end) = skip_jsx_element(s, rest_start) {
                            i = end;
                            continue;
                        }
                    }
                } else {
                    pending += 1;
                }
            } else if c == b':' {
                if let Some(qi) = q_idx {
                    if pending == 0 {
                        return Some((qi, i));
                    }
                    pending -= 1;
                }
            }
        }
        i += 1;
    }
    None
}

/// From a `<` at `start`, scans past the root JSX element's closing tag
/// (handling self-closing `<.../>`, nested same/different tags, attr-expr
/// braces, and quoted attr values) and returns the index just past it.
/// Returns `None` if the element never closes (malformed input — the
/// caller's normal scan will run to the end and yield no match).
fn skip_jsx_element(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    debug_assert_eq!(bytes.get(start), Some(&b'<'));
    let mut i = start;
    let mut tag_depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut brace_depth = 0i32;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if brace_depth > 0 {
            match c {
                b'\'' | b'"' | b'`' => quote = Some(c),
                b'{' => brace_depth += 1,
                b'}' => brace_depth -= 1,
                _ => {}
            }
            i += 1;
            continue;
        }
        match c {
            b'{' => {
                brace_depth += 1;
                i += 1;
            }
            b'"' | b'\'' | b'`' => {
                quote = Some(c);
                i += 1;
            }
            b'<' => {
                if bytes.get(i + 1) == Some(&b'/') {
                    tag_depth -= 1;
                    // skip to the matching '>' of this closing tag
                    while i < bytes.len() && bytes[i] != b'>' {
                        i += 1;
                    }
                    i += 1;
                    if tag_depth == 0 {
                        return Some(i);
                    }
                } else {
                    // opening (or self-closing) tag: scan to its '>'
                    let tag_start = i;
                    i += 1;
                    let mut inner_brace = 0i32;
                    let mut inner_quote: Option<u8> = None;
                    while i < bytes.len() {
                        let ic = bytes[i];
                        if let Some(q) = inner_quote {
                            if ic == q {
                                inner_quote = None;
                            }
                            i += 1;
                            continue;
                        }
                        match ic {
                            b'"' | b'\'' | b'`' => inner_quote = Some(ic),
                            b'{' => inner_brace += 1,
                            b'}' => inner_brace -= 1,
                            b'>' if inner_brace == 0 => break,
                            _ => {}
                        }
                        i += 1;
                    }
                    let self_closing = i > tag_start && bytes.get(i - 1) == Some(&b'/');
                    i += 1;
                    if !self_closing {
                        tag_depth += 1;
                    } else if tag_depth == 0 {
                        return Some(i);
                    }
                }
            }
            _ => {
                i += 1;
            }
        }
    }
    None
}

fn find_top_level(s: &str, needle: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let nb = needle.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'\'' | b'"' | b'`' => quote = Some(c),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            _ => {}
        }
        if depth == 0 && bytes[i..].starts_with(nb) {
            return Some(i);
        }
        i += 1;
    }
    None
}
