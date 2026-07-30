#[derive(Debug)]
pub enum Node {
    Element { tag: String, attrs: Vec<Attr>, children: Vec<Node> },
    Text(String),
    Expr(String),
    For { list: String, param: String, key: Option<String>, children: Vec<Node> },
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
                    if let Some(tag) = closing_tag {
                        return Err(format!(
                            "M1010 at line {}: unexpected EOF, expected </{}>",
                            self.line(),
                            tag
                        ));
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
                    flush_text(&mut text, &mut nodes);
                    let inner = self.read_braced()?;
                    nodes.push(classify_expr(&inner)?);
                }
                Some(c) => {
                    text.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    fn parse_element(&mut self) -> Result<Node, String> {
        self.pos += 1;
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
            self.pos += 1;
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
        let mut depth = 0usize;
        let mut out = String::new();
        let mut quote: Option<char> = None;
        loop {
            let c = self.peek().ok_or("unbalanced '{'")?;
            self.pos += 1;
            if let Some(q) = quote {
                out.push(c);
                if c == q {
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

fn classify_expr(inner: &str) -> Result<Node, String> {
    if let Some(node) = try_parse_map(inner)? {
        return Ok(node);
    }
    if let Some(node) = try_parse_logical_and(inner)? {
        return Ok(node);
    }
    Ok(Node::Expr(inner.to_string()))
}

fn try_parse_map(inner: &str) -> Result<Option<Node>, String> {
    let Some(map_idx) = find_top_level(inner, ".map(") else {
        return Ok(None);
    };
    let list = inner[..map_idx].trim().to_string();
    let after = &inner[map_idx + 5..];
    let Some(arrow_idx) = after.find("=>") else {
        return Ok(None);
    };
    let param = after[..arrow_idx].trim().trim_matches(|c| c == '(' || c == ')').trim().to_string();
    let mut body = after[arrow_idx + 2..].trim();
    body = body.strip_suffix(')').ok_or("expected ')' closing .map()")?.trim();
    if body.starts_with('(') && body.ends_with(')') {
        body = body[1..body.len() - 1].trim();
    }
    if !body.starts_with('<') {
        return Ok(None);
    }
    let mut children = parse(body)?;
    let key = extract_key(&mut children, &param)?;
    Ok(Some(Node::For { list, param, key, children }))
}

fn try_parse_logical_and(inner: &str) -> Result<Option<Node>, String> {
    let Some(idx) = find_top_level(inner, "&&") else {
        return Ok(None);
    };
    let mut rest = inner[idx + 2..].trim();
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
fn extract_key(children: &mut [Node], param: &str) -> Result<Option<String>, String> {
    for node in children.iter_mut() {
        if let Node::Element { attrs, .. } = node {
            if let Some(i) = attrs.iter().position(|a| a.name == "key") {
                let attr = attrs.remove(i);
                if let AttrValue::Expr(e) = attr.value {
                    let e = e.trim().to_string();
                    if e == "index" {
                        return Ok(None);
                    }
                    if e == param {
                        return Ok(Some("*this".to_string()));
                    }
                    if let Some(prop) = e.strip_prefix(&format!("{}.", param)) {
                        if !prop.is_empty()
                            && prop.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
                        {
                            return Ok(Some(prop.to_string()));
                        }
                    }
                    return Err(format!(
                        "M1003: key={{{}}} — wx:key needs a direct property of `{}` (e.g. key={{{}.id}}), `{}` itself, or `index`",
                        e, param, param, param
                    ));
                }
                return Err(format!(
                    "M1003: key must be an expression like key={{{}.id}}",
                    param
                ));
            }
        }
    }
    Ok(None)
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
