use std::collections::HashMap;
use std::path::PathBuf;

use regex::Regex;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

struct Backend {
    client: Client,
    docs: RwLock<HashMap<Url, String>>,
}

fn doc_dir(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()?.parent().map(|p| p.to_path_buf())
}

fn is_page(uri: &Url) -> bool {
    uri.path().contains("/pages/")
}

fn is_app(uri: &Url) -> bool {
    uri.path().ends_with("/app.mist")
}

fn line_range(src: &str, line: usize, col: usize) -> Range {
    let line0 = line.saturating_sub(1) as u32;
    let text = src.lines().nth(line.saturating_sub(1)).unwrap_or("");
    let mut byte_col = col.saturating_sub(1).min(text.len());
    while !text.is_char_boundary(byte_col) {
        byte_col -= 1;
    }
    let start = text[..byte_col].encode_utf16().count() as u32;
    let end = (text.encode_utf16().count() as u32).max(start + 1);
    Range {
        start: Position { line: line0, character: start },
        end: Position { line: line0, character: end },
    }
}

fn error_to_diagnostics(src: &str, err: &str) -> Vec<Diagnostic> {
    let head = Regex::new(r"(M\d{4}) at line (\d+)(?::(\d+))?").unwrap();
    let starts: Vec<usize> = head.find_iter(err).map(|m| m.start()).collect();
    if starts.is_empty() {
        return vec![Diagnostic {
            range: Range::default(),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("mistc".into()),
            message: err.trim().trim_end_matches(';').to_string(),
            ..Default::default()
        }];
    }
    let mut out = Vec::new();
    for (i, &start) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(err.len());
        let segment = err[start..end].trim().trim_end_matches("; ").trim_end_matches(';');
        let caps = head.captures(segment).unwrap();
        let code = caps.get(1).unwrap().as_str().to_string();
        let line: usize = caps[2].parse().unwrap_or(1);
        let col: usize = caps.get(3).map(|c| c.as_str().parse().unwrap_or(1)).unwrap_or(1);
        out.push(Diagnostic {
            range: line_range(src, line, col),
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String(code)),
            source: Some("mistc".into()),
            message: segment.to_string(),
            ..Default::default()
        });
    }
    out
}

fn diagnostics_for(uri: &Url, src: &str) -> Vec<Diagnostic> {
    let dir = doc_dir(uri);
    let resolver = |import_path: &str| -> Option<mistc::frontmatter::StoreModuleInfo> {
        let store_src = std::fs::read_to_string(dir.as_ref()?.join(import_path)).ok()?;
        mistc::frontmatter::store_module_info(&store_src).ok()
    };
    let result = if is_app(uri) {
        match mistc::sfc::split(src) {
            Ok(sfc) => mistc::frontmatter::analyze(sfc.frontmatter).map(|_| ()),
            Err(e) => Err(e),
        }
    } else {
        mistc::compile_unit_with_stores(src, is_page(uri), &resolver).map(|_| ())
    };
    match result {
        Ok(()) => Vec::new(),
        Err(e) => error_to_diagnostics(src, &e),
    }
}

fn ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

fn byte_col_at(text: &str, utf16_col: u32) -> usize {
    let mut acc: u32 = 0;
    for (i, c) in text.char_indices() {
        if acc >= utf16_col {
            return i;
        }
        acc += c.len_utf16() as u32;
    }
    text.len()
}

fn word_at(src: &str, position: Position) -> Option<String> {
    let line = src.lines().nth(position.line as usize)?;
    let mut b = byte_col_at(line, position.character);
    if b >= line.len() || !ident_char(line[b..].chars().next()?) {
        let prev_ok = line[..b].chars().next_back().map(ident_char).unwrap_or(false);
        if !prev_ok {
            return None;
        }
        b -= line[..b].chars().next_back()?.len_utf8();
    }
    let start = line[..b]
        .char_indices()
        .rev()
        .take_while(|(_, c)| ident_char(*c))
        .last()
        .map(|(i, _)| i)
        .unwrap_or(b);
    let end = line[b..]
        .char_indices()
        .take_while(|(_, c)| ident_char(*c))
        .last()
        .map(|(i, c)| b + i + c.len_utf8())
        .unwrap_or(b);
    if start >= end {
        return None;
    }
    Some(line[start..end].to_string())
}

fn pos_of_byte(src: &str, byte: usize) -> Position {
    let prefix = &src[..byte.min(src.len())];
    let line = prefix.matches('\n').count() as u32;
    let line_start = prefix.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let character = src[line_start..byte.min(src.len())].encode_utf16().count() as u32;
    Position { line, character }
}

fn ident_occurrences(src: &str, name: &str) -> Vec<Range> {
    let mut out = Vec::new();
    for (i, _) in src.match_indices(name) {
        let before_ok = src[..i].chars().next_back().map(|c| !ident_char(c)).unwrap_or(true);
        let after_ok = src[i + name.len()..].chars().next().map(|c| !ident_char(c)).unwrap_or(true);
        if before_ok && after_ok {
            out.push(Range { start: pos_of_byte(src, i), end: pos_of_byte(src, i + name.len()) });
        }
    }
    out
}

fn find_decl(source: &str, name: &str) -> Option<usize> {
    let re = Regex::new(&format!(
        r"(?:const|let|function)\s+({})[^A-Za-z0-9_$]",
        regex::escape(name)
    ))
    .ok()?;
    re.captures(source).and_then(|c| c.get(1)).map(|m| m.start())
}

fn find_prop_decl(source: &str, name: &str) -> Option<usize> {
    let re = Regex::new(r"const\s*\{([^}]*)\}\s*=\s*props\(").ok()?;
    let inner = re.captures(source)?.get(1)?;
    for (i, _) in inner.as_str().match_indices(name) {
        let before_ok =
            inner.as_str()[..i].chars().next_back().map(|c| !ident_char(c)).unwrap_or(true);
        let after_ok = inner.as_str()[i + name.len()..]
            .chars()
            .next()
            .map(|c| !ident_char(c))
            .unwrap_or(true);
        if before_ok && after_ok {
            return Some(inner.start() + i);
        }
    }
    None
}

fn find_store_decl(source: &str, name: &str) -> Option<usize> {
    let re = Regex::new(&format!(
        r"export\s+(?:const|function)\s+({})[^A-Za-z0-9_$]",
        regex::escape(name)
    ))
    .ok()?;
    re.captures(source)
        .and_then(|c| c.get(1))
        .map(|m| m.start())
        .or_else(|| find_decl(source, name))
}

fn derived_source(frontmatter: &str, name: &str) -> Option<String> {
    let re = Regex::new(&format!(r"\b{}\s*=\s*derived\(", regex::escape(name))).ok()?;
    let start = re.find(frontmatter)?.end();
    let mut depth = 1;
    for (i, c) in frontmatter[start..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(frontmatter[start..start + i].trim().to_string());
                }
            }
            _ => {}
        }
    }
    None
}

struct Symbol {
    name: String,
    hover: String,
    params: Option<String>,
    def: Option<Range>,
    def_file: Option<(PathBuf, Range)>,
    local: bool,
}

fn frontmatter_def(src: &str, frontmatter: &str, name: &str) -> Option<Range> {
    let idx = find_decl(frontmatter, name).or_else(|| find_prop_decl(frontmatter, name))?;
    let fm_start = src.find(frontmatter)?;
    let start = pos_of_byte(src, fm_start + idx);
    let end = pos_of_byte(src, fm_start + idx + name.len());
    Some(Range { start, end })
}

fn symbols_for(uri: &Url, src: &str) -> Option<Vec<Symbol>> {
    let sfc = mistc::sfc::split(src).ok()?;
    let dir = doc_dir(uri);
    let resolver = |import_path: &str| -> Option<mistc::frontmatter::StoreModuleInfo> {
        let store_src = std::fs::read_to_string(dir.as_ref()?.join(import_path)).ok()?;
        mistc::frontmatter::store_module_info(&store_src).ok()
    };
    let analysis =
        mistc::frontmatter::analyze_with_stores(sfc.frontmatter, &resolver, sfc.frontmatter_line)
            .ok()?;
    let mut out = Vec::new();
    for s in &analysis.states {
        out.push(Symbol {
            name: s.name.clone(),
            hover: format!("**{}** — state\n\n```ts\nstate({})\n```", s.name, s.init),
            params: None,
            def: frontmatter_def(src, sfc.frontmatter, &s.name),
            def_file: None,
            local: true,
        });
    }
    for d in &analysis.deriveds {
        let arrow = derived_source(sfc.frontmatter, &d.name).unwrap_or_else(|| d.arrow.clone());
        out.push(Symbol {
            name: d.name.clone(),
            hover: format!("**{}** — derived\n\n```ts\n{}\n```", d.name, arrow),
            params: None,
            def: frontmatter_def(src, sfc.frontmatter, &d.name),
            def_file: None,
            local: true,
        });
    }
    for m in &analysis.methods {
        let params = m
            .params
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .trim()
            .to_string();
        out.push(Symbol {
            name: m.name.clone(),
            hover: format!("**{}**({}) — method", m.name, params),
            params: Some(params),
            def: frontmatter_def(src, sfc.frontmatter, &m.name),
            def_file: None,
            local: true,
        });
    }
    for p in &analysis.data_props {
        let default = p
            .default
            .as_ref()
            .map(|d| format!(" (default: `{}`)", d))
            .unwrap_or_default();
        out.push(Symbol {
            name: p.name.clone(),
            hover: format!("**{}** — prop{}", p.name, default),
            params: None,
            def: frontmatter_def(src, sfc.frontmatter, &p.name),
            def_file: None,
            local: true,
        });
    }
    for c in &analysis.callback_props {
        out.push(Symbol {
            name: c.clone(),
            hover: format!("**{}** — callback prop", c),
            params: None,
            def: frontmatter_def(src, sfc.frontmatter, c),
            def_file: None,
            local: true,
        });
    }
    for si in &analysis.store_imports {
        let store_path = dir.as_ref().map(|d| d.join(&si.path));
        let store_src = store_path.as_ref().and_then(|p| std::fs::read_to_string(p).ok());
        let def_in_store = |name: &str| -> Option<(PathBuf, Range)> {
            let text = store_src.as_ref()?;
            let idx = find_store_decl(text, name)?;
            Some((
                store_path.clone()?,
                Range { start: pos_of_byte(text, idx), end: pos_of_byte(text, idx + name.len()) },
            ))
        };
        for s in &si.stores {
            out.push(Symbol {
                name: s.clone(),
                hover: format!("**{}** — store (from `{}`)", s, si.path),
                params: None,
                def: None,
                def_file: def_in_store(s),
                local: false,
            });
        }
        for f in &si.fns {
            out.push(Symbol {
                name: f.clone(),
                hover: format!("**{}** — store function (from `{}`)", f, si.path),
                params: None,
                def: None,
                def_file: def_in_store(f),
                local: false,
            });
        }
    }
    Some(out)
}

fn call_context(src: &str, position: Position) -> Option<(String, u32)> {
    let line = src.lines().nth(position.line as usize)?;
    let cursor = byte_col_at(line, position.character);
    let mut depth = 0;
    let mut open = None;
    for (i, c) in line[..cursor].char_indices().rev() {
        match c {
            ')' => depth += 1,
            '(' if depth == 0 => {
                open = Some(i);
                break;
            }
            '(' => depth -= 1,
            _ => {}
        }
    }
    let open = open?;
    let name_end = open;
    let name_start = line[..name_end]
        .char_indices()
        .rev()
        .take_while(|(_, c)| ident_char(*c))
        .last()
        .map(|(i, _)| i)?;
    let name = line[name_start..name_end].to_string();
    let mut commas = 0;
    let mut d = 0;
    for c in line[open + 1..cursor].chars() {
        match c {
            '(' | '[' | '{' => d += 1,
            ')' | ']' | '}' => d -= 1,
            ',' if d == 0 => commas += 1,
            _ => {}
        }
    }
    Some((name, commas))
}

fn completions_for(uri: &Url, src: &str, position: Position) -> Option<Vec<CompletionItem>> {
    let sfc = mistc::sfc::split(src).ok()?;
    if (position.line as usize + 1) < sfc.template_line {
        return None;
    }
    let dir = doc_dir(uri);
    let resolver = |import_path: &str| -> Option<mistc::frontmatter::StoreModuleInfo> {
        let store_src = std::fs::read_to_string(dir.as_ref()?.join(import_path)).ok()?;
        mistc::frontmatter::store_module_info(&store_src).ok()
    };
    let analysis =
        mistc::frontmatter::analyze_with_stores(sfc.frontmatter, &resolver, sfc.frontmatter_line)
            .ok()?;
    let mut items = Vec::new();
    for s in &analysis.states {
        items.push(CompletionItem {
            label: s.name.clone(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some("state".into()),
            insert_text: Some(format!("{}.value", s.name)),
            ..Default::default()
        });
    }
    for d in &analysis.deriveds {
        items.push(CompletionItem {
            label: d.name.clone(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some("derived".into()),
            insert_text: Some(format!("{}.value", d.name)),
            ..Default::default()
        });
    }
    for m in &analysis.methods {
        items.push(CompletionItem {
            label: m.name.clone(),
            kind: Some(CompletionItemKind::METHOD),
            detail: Some("method".into()),
            ..Default::default()
        });
    }
    for si in &analysis.store_imports {
        for s in &si.stores {
            items.push(CompletionItem {
                label: s.clone(),
                kind: Some(CompletionItemKind::VARIABLE),
                detail: Some("store".into()),
                insert_text: Some(format!("{}.value", s)),
                ..Default::default()
            });
        }
        for f in &si.fns {
            items.push(CompletionItem {
                label: f.clone(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some("store fn".into()),
                ..Default::default()
            });
        }
    }
    Some(items)
}

impl Backend {
    async fn publish(&self, uri: Url, src: String) {
        let diagnostics = diagnostics_for(&uri, &src);
        self.docs.write().await.insert(uri.clone(), src);
        self.client.publish_diagnostics(uri, diagnostics, None).await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["{".into(), ".".into()]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".into(), ",".into()]),
                    ..Default::default()
                }),
                rename_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "mistc-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client.log_message(MessageType::INFO, "mistc-lsp ready").await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.publish(params.text_document.uri, params.text_document.text).await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.pop() {
            self.publish(params.text_document.uri, change.text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.docs.write().await.remove(&params.text_document.uri);
        self.client.publish_diagnostics(params.text_document.uri, Vec::new(), None).await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let docs = self.docs.read().await;
        let Some(src) = docs.get(&uri) else { return Ok(None) };
        Ok(completions_for(&uri, src, position).map(CompletionResponse::Array))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let docs = self.docs.read().await;
        let Some(src) = docs.get(&uri) else { return Ok(None) };
        let Some(word) = word_at(src, position) else { return Ok(None) };
        let Some(symbols) = symbols_for(&uri, src) else { return Ok(None) };
        let Some(symbol) = symbols.iter().find(|s| s.name == word) else { return Ok(None) };
        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: symbol.hover.clone(),
            }),
            range: None,
        }))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let docs = self.docs.read().await;
        let Some(src) = docs.get(&uri) else { return Ok(None) };
        let Some(word) = word_at(src, position) else { return Ok(None) };
        let Some(symbols) = symbols_for(&uri, src) else { return Ok(None) };
        let Some(symbol) = symbols.iter().find(|s| s.name == word) else { return Ok(None) };
        if let Some(range) = symbol.def {
            return Ok(Some(GotoDefinitionResponse::Scalar(Location { uri, range })));
        }
        if let Some((path, range)) = &symbol.def_file {
            if let Ok(file_uri) = Url::from_file_path(path) {
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: file_uri,
                    range: *range,
                })));
            }
        }
        Ok(None)
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let docs = self.docs.read().await;
        let Some(src) = docs.get(&uri) else { return Ok(None) };
        let Some((name, active)) = call_context(src, position) else { return Ok(None) };
        let Some(symbols) = symbols_for(&uri, src) else { return Ok(None) };
        let Some(symbol) = symbols.iter().find(|s| s.name == name) else { return Ok(None) };
        let Some(params_str) = &symbol.params else { return Ok(None) };
        let parameters: Vec<ParameterInformation> = params_str
            .split(',')
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .map(|p| ParameterInformation {
                label: ParameterLabel::Simple(p.to_string()),
                documentation: None,
            })
            .collect();
        Ok(Some(SignatureHelp {
            signatures: vec![SignatureInformation {
                label: format!("{}({})", name, params_str),
                documentation: None,
                parameters: Some(parameters),
                active_parameter: None,
            }],
            active_signature: Some(0),
            active_parameter: Some(active),
        }))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;
        let docs = self.docs.read().await;
        let Some(src) = docs.get(&uri) else { return Ok(None) };
        let Some(word) = word_at(src, position) else { return Ok(None) };
        let Some(symbols) = symbols_for(&uri, src) else { return Ok(None) };
        let Some(symbol) = symbols.iter().find(|s| s.name == word) else { return Ok(None) };
        if !symbol.local {
            return Err(tower_lsp::jsonrpc::Error::invalid_params(
                "store symbols are defined in their module — rename there",
            ));
        }
        let valid = !new_name.is_empty()
            && new_name.chars().next().map(|c| c.is_alphabetic() || c == '_' || c == '$').unwrap_or(false)
            && new_name.chars().all(ident_char);
        if !valid {
            return Err(tower_lsp::jsonrpc::Error::invalid_params("not a valid identifier"));
        }
        if symbols.iter().any(|s| s.name == new_name) {
            return Err(tower_lsp::jsonrpc::Error::invalid_params(
                "name already used by another state/derived/method/prop",
            ));
        }
        let edits: Vec<TextEdit> = ident_occurrences(src, &word)
            .into_iter()
            .map(|range| TextEdit { range, new_text: new_name.clone() })
            .collect();
        if edits.is_empty() {
            return Ok(None);
        }
        let mut changes = HashMap::new();
        changes.insert(uri, edits);
        Ok(Some(WorkspaceEdit { changes: Some(changes), ..Default::default() }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_range_converts_byte_cols_to_utf16() {
        let src = "let s = '中文'\nlet 名字 = x.value\n";
        let r = line_range(src, 2, 12);
        assert_eq!(r.start, Position { line: 1, character: 7 });
        assert_eq!(r.end, Position { line: 1, character: 16 });
    }

    #[test]
    fn line_range_clamps_out_of_range() {
        let r = line_range("short\n", 99, 99);
        assert_eq!(r.start, Position { line: 98, character: 0 });
        assert_eq!(r.end.character, 1);
    }

    #[test]
    fn error_string_splits_into_coded_diagnostics() {
        let src = "a\nbb\nccc\n";
        let err = "M1004 at line 2:1: bad\n  help: fix; M1001 at line 3:2: worse\n  help: fix2";
        let ds = error_to_diagnostics(src, err);
        assert_eq!(ds.len(), 2);
        assert_eq!(ds[0].code, Some(NumberOrString::String("M1004".into())));
        assert_eq!(ds[0].range.start, Position { line: 1, character: 0 });
        assert_eq!(ds[1].code, Some(NumberOrString::String("M1001".into())));
        assert_eq!(ds[1].range.start, Position { line: 2, character: 1 });
        assert!(ds[1].message.contains("help: fix2"));
    }

    #[test]
    fn uncoded_error_lands_at_origin() {
        let ds = error_to_diagnostics("x\n", "expected frontmatter opening '---'");
        assert_eq!(ds.len(), 1);
        assert_eq!(ds[0].range, Range::default());
    }

    #[test]
    fn word_at_finds_identifiers() {
        let src = "abc\n{toggle(0)}\n";
        assert_eq!(word_at(src, Position { line: 1, character: 3 }), Some("toggle".into()));
        assert_eq!(word_at(src, Position { line: 1, character: 7 }), Some("toggle".into()));
        assert_eq!(word_at(src, Position { line: 1, character: 0 }), None);
        assert_eq!(word_at(src, Position { line: 9, character: 0 }), None);
    }

    #[test]
    fn ident_occurrences_are_whole_word() {
        let src = "open opened {open.value} reopen\n";
        let occ = ident_occurrences(src, "open");
        assert_eq!(occ.len(), 2);
        assert_eq!(occ[0].start.character, 0);
        assert_eq!(occ[1].start.character, 13);
    }

    #[test]
    fn find_decl_locates_declarations() {
        let fm = "\nimport { state } from 'mist'\nconst todos = state([])\nfunction toggle(i) {}\n";
        assert!(find_decl(fm, "todos").is_some());
        assert!(find_decl(fm, "toggle").is_some());
        assert_eq!(find_decl(fm, "missing"), None);
        let idx = find_decl(fm, "todos").unwrap();
        assert_eq!(&fm[idx..idx + 5], "todos");
    }

    #[test]
    fn find_prop_decl_matches_destructured_props() {
        let fm = "\nimport { props } from 'mist'\nconst { title, onPing } = props()\n";
        let idx = find_prop_decl(fm, "title").unwrap();
        assert_eq!(&fm[idx..idx + 5], "title");
        let idx = find_prop_decl(fm, "onPing").unwrap();
        assert_eq!(&fm[idx..idx + 6], "onPing");
        assert_eq!(find_prop_decl(fm, "tit"), None);
        assert_eq!(find_prop_decl(fm, "missing"), None);
    }

    #[test]
    fn call_context_reports_name_and_active_param() {
        let src = "{toggle(1, f(2), )}\n";
        assert_eq!(call_context(src, Position { line: 0, character: 9 }), Some(("toggle".into(), 0)));
        assert_eq!(call_context(src, Position { line: 0, character: 17 }), Some(("toggle".into(), 2)));
        assert_eq!(call_context(src, Position { line: 0, character: 14 }), Some(("f".into(), 0)));
        assert_eq!(call_context(src, Position { line: 0, character: 1 }), None);
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) =
        LspService::new(|client| Backend { client, docs: RwLock::new(HashMap::new()) });
    Server::new(stdin, stdout, socket).serve(service).await;
}
