use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use regex::Regex;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

struct Backend {
    client: Client,
    docs: Arc<RwLock<HashMap<Url, String>>>,
    diag_gen: Arc<RwLock<HashMap<Url, u64>>>,
    gen_counter: AtomicU64,
    pending_watched: Arc<RwLock<Vec<PathBuf>>>,
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
    if is_app(uri) {
        return match mistc::sfc::split(src) {
            Ok(sfc) => match mistc::frontmatter::analyze(sfc.frontmatter) {
                Ok(_) => Vec::new(),
                Err(e) => error_to_diagnostics(src, &e),
            },
            Err(e) => error_to_diagnostics(src, &e),
        };
    }
    match mistc::compile_unit_with_stores(src, is_page(uri), &resolver) {
        Ok(unit) => {
            let mut out = missing_component_diagnostics(src, dir.as_deref(), &unit);
            out.extend(warning_diagnostics(src, &unit.warnings));
            out
        }
        Err(e) => error_to_diagnostics(src, &e),
    }
}

fn warning_diagnostics(src: &str, warnings: &[String]) -> Vec<Diagnostic> {
    let code_re = Regex::new(r"^(M\d{4}):").unwrap();
    let token_re = Regex::new(r"`([^`\n]+)`|'([^'\n]+)'|<([a-z][a-z0-9-]*)>").unwrap();
    warnings
        .iter()
        .map(|w| {
            let code = code_re.captures(w).map(|c| c[1].to_string());
            let range = token_re
                .captures(w)
                .and_then(|c| c.get(1).or_else(|| c.get(2)).or_else(|| c.get(3)))
                .and_then(|m| {
                    let needle = m.as_str();
                    src.find(needle).map(|i| Range {
                        start: pos_of_byte(src, i),
                        end: pos_of_byte(src, i + needle.len()),
                    })
                })
                .unwrap_or_default();
            Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::WARNING),
                code: code.map(NumberOrString::String),
                source: Some("mistc".into()),
                message: w.clone(),
                ..Default::default()
            }
        })
        .collect()
}

fn config_insert_edit(src: &str, entry: &str) -> TextEdit {
    if let Some(i) = src.find("export const config = {") {
        let at = i + "export const config = {".len();
        return TextEdit {
            range: Range { start: pos_of_byte(src, at), end: pos_of_byte(src, at) },
            new_text: format!(" {},", entry),
        };
    }
    let at = src.find("---\n").map(|i| i + 4).unwrap_or(0);
    TextEdit {
        range: Range { start: pos_of_byte(src, at), end: pos_of_byte(src, at) },
        new_text: format!("export const config = {{ {} }}\n", entry),
    }
}

fn quick_fix(title: &str, uri: &Url, diag: &Diagnostic, edits: Vec<TextEdit>) -> CodeActionOrCommand {
    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), edits);
    CodeActionOrCommand::CodeAction(CodeAction {
        title: title.to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diag.clone()]),
        edit: Some(WorkspaceEdit { changes: Some(changes), ..Default::default() }),
        ..Default::default()
    })
}

fn quick_fixes(src: &str, uri: &Url, diag: &Diagnostic) -> Vec<CodeActionOrCommand> {
    let code = match &diag.code {
        Some(NumberOrString::String(c)) => c.as_str(),
        _ => return Vec::new(),
    };
    let msg = &diag.message;
    let mut out = Vec::new();
    match code {
        "M1007" => {
            let Some(name) = Regex::new(r"`([^`]+)` is reactive").unwrap().captures(msg).map(|c| c[1].to_string()) else {
                return out;
            };
            let byte = position_to_byte(src, diag.range.start);
            if src[byte..].starts_with(&name) {
                let range = Range {
                    start: pos_of_byte(src, byte),
                    end: pos_of_byte(src, byte + name.len()),
                };
                out.push(quick_fix(
                    &format!("Change to `{}.value`", name),
                    uri,
                    diag,
                    vec![TextEdit { range, new_text: format!("{}.value", name) }],
                ));
            }
        }
        "M1023" | "M1024" => {
            let name = Regex::new(r"add '([^']+)' to config\.customAttrs")
                .unwrap()
                .captures(msg)
                .map(|c| c[1].to_string());
            let Some(name) = name else { return out };
            if let Some(sug) = Regex::new(r"did you mean ([A-Za-z0-9:_-]+)\?")
                .unwrap()
                .captures(msg)
                .map(|c| c[1].to_string())
            {
                if let Some(i) = src.find(&name) {
                    let range = Range {
                        start: pos_of_byte(src, i),
                        end: pos_of_byte(src, i + name.len()),
                    };
                    out.push(quick_fix(
                        &format!("Rename to `{}`", sug),
                        uri,
                        diag,
                        vec![TextEdit { range, new_text: sug }],
                    ));
                }
            }
            let edit = if let Some(i) = src.find("customAttrs: [") {
                let at = i + "customAttrs: [".len();
                TextEdit {
                    range: Range { start: pos_of_byte(src, at), end: pos_of_byte(src, at) },
                    new_text: format!("'{}', ", name),
                }
            } else {
                config_insert_edit(src, &format!("customAttrs: ['{}']", name))
            };
            out.push(quick_fix(
                &format!("Add '{}' to config.customAttrs", name),
                uri,
                diag,
                vec![edit],
            ));
        }
        "M1027" => {
            let Some(since) = Regex::new(r"≥ ([0-9.]+)").unwrap().captures(msg).map(|c| c[1].to_string()) else {
                return out;
            };
            let edit = if let Some(i) = src.find("minLibVersion: '") {
                let at = i + "minLibVersion: '".len();
                let end = src[at..].find('\'').map(|j| at + j).unwrap_or(at);
                TextEdit {
                    range: Range { start: pos_of_byte(src, at), end: pos_of_byte(src, end) },
                    new_text: since.clone(),
                }
            } else {
                config_insert_edit(src, &format!("minLibVersion: '{}'", since))
            };
            out.push(quick_fix(&format!("Set minLibVersion to '{}'", since), uri, diag, vec![edit]));
        }
        "M1026" => {
            let Some(name) = Regex::new(r"reactive value '([^']+)'").unwrap().captures(msg).map(|c| c[1].to_string()) else {
                return out;
            };
            let byte = position_to_byte(src, diag.range.start);
            let needle = format!("{}.value", name);
            let found = src[byte..].find(&needle).map(|i| byte + i).or_else(|| src.find(&needle));
            let Some(i) = found else { return out };
            if src[..i].trim_end().ends_with("raw(") {
                return out;
            }
            let mut edits = vec![TextEdit {
                range: Range { start: pos_of_byte(src, i), end: pos_of_byte(src, i + needle.len()) },
                new_text: format!("raw({})", needle),
            }];
            let import_re = Regex::new(r"import\s*\{([^}]*)\}\s*from\s*'mist'").unwrap();
            if let Some(c) = import_re.captures(src) {
                let inner = c.get(1).unwrap();
                if !inner.as_str().split(',').any(|s| s.trim() == "raw") {
                    let at = inner.start();
                    edits.push(TextEdit {
                        range: Range { start: pos_of_byte(src, at), end: pos_of_byte(src, at) },
                        new_text: " raw,".to_string(),
                    });
                }
            }
            out.push(quick_fix("Wrap in raw() — conservative re-sync after the call", uri, diag, edits));
        }
        _ => {}
    }
    out
}

fn missing_component_diagnostics(
    src: &str,
    dir: Option<&Path>,
    unit: &mistc::Unit,
) -> Vec<Diagnostic> {
    let Some(dir) = dir else { return Vec::new() };
    let mut out = Vec::new();
    for (_, path) in &unit.used_imports {
        if !path.ends_with(".mist") || dir.join(path).exists() {
            continue;
        }
        let needle = format!("'{}'", path);
        let range = src
            .find(&needle)
            .map(|i| Range { start: pos_of_byte(src, i), end: pos_of_byte(src, i + needle.len()) })
            .unwrap_or_default();
        out.push(Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("mistc".into()),
            message: format!("component import '{}' not found — was the file moved or deleted?", path),
            ..Default::default()
        });
    }
    out
}

fn ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

fn position_to_byte(src: &str, pos: Position) -> usize {
    let mut line_start = 0;
    for _ in 0..pos.line {
        match src[line_start..].find('\n') {
            Some(i) => line_start += i + 1,
            None => return src.len(),
        }
    }
    let line_end = src[line_start..].find('\n').map(|i| line_start + i).unwrap_or(src.len());
    line_start + byte_col_at(&src[line_start..line_end], pos.character)
}

fn apply_change(src: &mut String, range: Option<Range>, text: &str) {
    match range {
        Some(range) => {
            let start = position_to_byte(src, range.start);
            let end = position_to_byte(src, range.end).max(start);
            src.replace_range(start..end, text);
        }
        None => *src = text.to_string(),
    }
}

fn project_src_root(dir: &Path) -> Option<PathBuf> {
    let mut cur = dir.to_path_buf();
    for _ in 0..8 {
        if cur.join("app.mist").is_file() {
            return Some(cur);
        }
        cur = cur.parent()?.to_path_buf();
    }
    None
}

fn collect_project_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else { continue };
        if file_type.is_symlink() {
            continue;
        }
        let p = entry.path();
        if file_type.is_dir() {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "node_modules" || name == "dist" || name.starts_with('.') {
                continue;
            }
            collect_project_files(&p, out);
        } else if p.extension().and_then(|e| e.to_str()).is_some_and(|e| e == "mist" || e == "ts")
        {
            out.push(p);
        }
    }
}

fn mist_decl_collision(
    files: &[(PathBuf, String)],
    changes: &HashMap<Url, Vec<TextEdit>>,
    new_name: &str,
) -> Option<PathBuf> {
    for (path, src) in files {
        if path.extension().and_then(|e| e.to_str()) != Some("mist") {
            continue;
        }
        let Ok(canon) = path.canonicalize() else { continue };
        let Ok(uri) = Url::from_file_path(&canon) else { continue };
        if changes.contains_key(&uri)
            && (find_decl(src, new_name).is_some() || find_prop_decl(src, new_name).is_some())
        {
            return Some(path.clone());
        }
    }
    None
}

fn imports_store(src: &str, file_dir: &Path, store_canon: &Path, name: &str) -> bool {
    let re = Regex::new(r#"import\s*\{([^}]*)\}\s*from\s*['"]([^'"]+)['"]"#).unwrap();
    for caps in re.captures_iter(src) {
        let names = caps.get(1).unwrap().as_str();
        let path = caps.get(2).unwrap().as_str();
        if !path.starts_with('.') {
            continue;
        }
        let Ok(canon) = file_dir.join(path).canonicalize() else { continue };
        if canon != store_canon {
            continue;
        }
        let listed = names.split(',').any(|n| n.trim() == name);
        if listed {
            return true;
        }
    }
    false
}

fn resolved_key(path: &Path) -> Option<PathBuf> {
    if let Ok(canon) = path.canonicalize() {
        return Some(canon);
    }
    let parent = path.parent()?.canonicalize().ok()?;
    Some(parent.join(path.file_name()?))
}

fn imports_path(src: &str, file_dir: &Path, target_key: &Path) -> bool {
    let re = Regex::new(r#"from\s*['"](\.[^'"]+)['"]"#).unwrap();
    let hit = re.captures_iter(src).any(|caps| {
        resolved_key(&file_dir.join(caps.get(1).unwrap().as_str()))
            .is_some_and(|key| key == target_key)
    });
    hit
}

fn project_files_with_open(root: &Path, docs: &HashMap<Url, String>) -> Vec<(PathBuf, String)> {
    let mut paths = Vec::new();
    collect_project_files(root, &mut paths);
    let mut open_by_path: HashMap<PathBuf, &String> = HashMap::new();
    for (uri, text) in docs.iter() {
        if let Ok(p) = uri.to_file_path() {
            if let Ok(canon) = p.canonicalize() {
                open_by_path.insert(canon, text);
            }
        }
    }
    let mut out = Vec::new();
    for p in paths {
        let canon = p.canonicalize().unwrap_or_else(|_| p.clone());
        let src = match open_by_path.get(&canon) {
            Some(text) => (*text).clone(),
            None => match std::fs::read_to_string(&p) {
                Ok(s) => s,
                Err(_) => continue,
            },
        };
        out.push((p, src));
    }
    out
}

fn store_rename_edits(
    store_canon: &Path,
    name: &str,
    new_name: &str,
    files: &[(PathBuf, String)],
) -> HashMap<Url, Vec<TextEdit>> {
    let mut changes = HashMap::new();
    for (path, src) in files {
        let Ok(canon) = path.canonicalize() else { continue };
        let is_store_file = canon == store_canon;
        let dir = path.parent().unwrap_or(Path::new("."));
        if !is_store_file && !imports_store(src, dir, store_canon, name) {
            continue;
        }
        let edits: Vec<TextEdit> = ident_occurrences(src, name)
            .into_iter()
            .map(|range| TextEdit { range, new_text: new_name.to_string() })
            .collect();
        if edits.is_empty() {
            continue;
        }
        let Ok(uri) = Url::from_file_path(&canon) else { continue };
        changes.insert(uri, edits);
    }
    changes
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

enum TemplateCtx {
    TagName,
    AttrPosition { tag: String },
    ClosingTag,
}

fn template_context(src: &str, position: Position) -> Option<TemplateCtx> {
    let cursor = position_to_byte(src, position);
    let before = &src[..cursor];
    let lt = before.rfind('<')?;
    let after_lt = &before[lt + 1..];
    if after_lt.starts_with('/') {
        return (!after_lt.contains('>')).then_some(TemplateCtx::ClosingTag);
    }
    if after_lt.starts_with('!') {
        return None;
    }
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for c in after_lt.chars() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '{' => depth += 1,
            '}' => depth -= 1,
            '>' if depth == 0 => return None,
            _ => {}
        }
    }
    if depth > 0 || quote.is_some() {
        return None;
    }
    if after_lt.trim_end().ends_with('/') {
        return None;
    }
    let tag_ok =
        |t: &str| !t.is_empty() && t.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_');
    match after_lt.find(char::is_whitespace) {
        None => {
            if !after_lt.is_empty() && !tag_ok(after_lt) {
                return None;
            }
            Some(TemplateCtx::TagName)
        }
        Some(ws) => {
            let tag = &after_lt[..ws];
            if !tag_ok(tag) {
                return None;
            }
            Some(TemplateCtx::AttrPosition { tag: tag.to_string() })
        }
    }
}

fn attr_item(name: &str, detail: &str) -> CompletionItem {
    CompletionItem {
        label: name.to_string(),
        kind: Some(CompletionItemKind::PROPERTY),
        detail: Some(detail.into()),
        ..Default::default()
    }
}

fn event_item(name: &str) -> CompletionItem {
    CompletionItem {
        label: format!("on{}", name),
        kind: Some(CompletionItemKind::EVENT),
        detail: Some("event".into()),
        ..Default::default()
    }
}

fn native_attr_completions(tag: &str) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = mistc::tag_meta::UNIVERSAL_ATTRS
        .iter()
        .map(|a| attr_item(a, "attribute"))
        .collect();
    items.push(attr_item("class:list", "conditional classes"));
    let native = mistc::wxml::alias_target(tag);
    if matches!(native, "input" | "textarea") {
        items.push(attr_item("value:bind", "two-way binding"));
    }
    if matches!(native, "switch" | "checkbox") {
        items.push(attr_item("checked:bind", "two-way binding"));
    }
    for e in mistc::tag_meta::COMMON_EVENTS {
        items.push(event_item(e));
    }
    if let Some(meta) = mistc::tag_meta::meta_for(native) {
        for a in meta.attrs {
            match mistc::tag_meta::since_of(native, a) {
                Some(since) => items.push(attr_item(a, &format!("attribute (≥{})", since))),
                None => items.push(attr_item(a, "attribute")),
            }
        }
        for e in meta.events {
            let label = format!("on{}", e);
            match mistc::tag_meta::since_of(native, &label) {
                Some(since) => items.push(CompletionItem {
                    label,
                    kind: Some(CompletionItemKind::EVENT),
                    detail: Some(format!("event (≥{})", since)),
                    ..Default::default()
                }),
                None => items.push(event_item(e)),
            }
        }
    }
    items
}

fn template_completions(
    uri: &Url,
    ctx: TemplateCtx,
    analysis: &mistc::frontmatter::Analysis,
    docs: &HashMap<Url, String>,
) -> Option<Vec<CompletionItem>> {
    match ctx {
        TemplateCtx::ClosingTag => Some(Vec::new()),
        TemplateCtx::TagName => {
            let mut items: Vec<CompletionItem> = mistc::tag_meta::TAG_META
                .iter()
                .map(|m| CompletionItem {
                    label: m.tag.to_string(),
                    kind: Some(CompletionItemKind::STRUCT),
                    detail: Some("native".into()),
                    ..Default::default()
                })
                .collect();
            for t in mistc::wxml::WEB_ALIAS_TAGS {
                items.push(CompletionItem {
                    label: t.to_string(),
                    kind: Some(CompletionItemKind::STRUCT),
                    detail: Some(format!("→ {}", mistc::wxml::alias_target(t))),
                    ..Default::default()
                });
            }
            for i in &analysis.imports {
                items.push(CompletionItem {
                    label: i.local.clone(),
                    kind: Some(CompletionItemKind::CLASS),
                    detail: Some("component".into()),
                    ..Default::default()
                });
            }
            Some(items)
        }
        TemplateCtx::AttrPosition { tag } => {
            if let Some(import) = analysis.imports.iter().find(|i| i.local == tag) {
                return component_prop_completions(uri, &import.path, docs);
            }
            Some(native_attr_completions(&tag))
        }
    }
}

fn open_or_disk(docs: &HashMap<Url, String>, path: &Path) -> Option<String> {
    let Some(canon) = path.canonicalize().ok() else {
        return std::fs::read_to_string(path).ok();
    };
    for (doc_uri, text) in docs {
        if let Ok(p) = doc_uri.to_file_path() {
            if p == canon || p.canonicalize().is_ok_and(|c| c == canon) {
                return Some(text.clone());
            }
        }
    }
    std::fs::read_to_string(path).ok()
}

fn component_prop_completions(
    uri: &Url,
    import_path: &str,
    docs: &HashMap<Url, String>,
) -> Option<Vec<CompletionItem>> {
    let dir = doc_dir(uri)?;
    let child_path = dir.join(import_path);
    let child_src = open_or_disk(docs, &child_path)?;
    let child_sfc = mistc::sfc::split(&child_src).ok()?;
    let child_dir = child_path.parent().map(|p| p.to_path_buf());
    let resolver = |p: &str| -> Option<mistc::frontmatter::StoreModuleInfo> {
        let store_src = open_or_disk(docs, &child_dir.as_ref()?.join(p))?;
        mistc::frontmatter::store_module_info(&store_src).ok()
    };
    let child = mistc::frontmatter::analyze_with_stores(
        child_sfc.frontmatter,
        &resolver,
        child_sfc.frontmatter_line,
    )
    .ok()?;
    let mut items: Vec<CompletionItem> = child
        .data_props
        .iter()
        .map(|p| attr_item(&p.name, "prop"))
        .collect();
    for c in &child.callback_props {
        items.push(CompletionItem {
            label: c.clone(),
            kind: Some(CompletionItemKind::EVENT),
            detail: Some("callback prop".into()),
            ..Default::default()
        });
    }
    items.push(attr_item("key", "list key"));
    Some(items)
}

fn completions_for(
    uri: &Url,
    src: &str,
    position: Position,
    docs: &HashMap<Url, String>,
) -> Option<Vec<CompletionItem>> {
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
    if let Some(t_ctx) = template_context(src, position) {
        return template_completions(uri, t_ctx, &analysis, docs);
    }
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

    async fn publish_debounced(&self, uri: Url, src: String) {
        let generation = self.gen_counter.fetch_add(1, Ordering::SeqCst) + 1;
        self.diag_gen.write().await.insert(uri.clone(), generation);
        let diag_gen = Arc::clone(&self.diag_gen);
        let client = self.client.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            if diag_gen.read().await.get(&uri) != Some(&generation) {
                return;
            }
            let diagnostics = diagnostics_for(&uri, &src);
            if diag_gen.read().await.get(&uri) != Some(&generation) {
                return;
            }
            client.publish_diagnostics(uri, diagnostics, None).await;
        });
    }

    async fn rediagnose_watched(&self, changed: Vec<PathBuf>) {
        let gen_key = Url::parse("mist://watched-files").unwrap();
        let generation = self.gen_counter.fetch_add(1, Ordering::SeqCst) + 1;
        self.diag_gen.write().await.insert(gen_key.clone(), generation);
        self.pending_watched.write().await.extend(changed);
        let diag_gen = Arc::clone(&self.diag_gen);
        let docs = Arc::clone(&self.docs);
        let pending = Arc::clone(&self.pending_watched);
        let client = self.client.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            if diag_gen.read().await.get(&gen_key) != Some(&generation) {
                return;
            }
            let snapshot: Vec<PathBuf> = pending.read().await.clone();
            let mut by_root: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
            for path in &snapshot {
                let Some(key) = resolved_key(path) else { continue };
                let Some(root) = path.parent().and_then(project_src_root) else { continue };
                let keys = by_root.entry(root).or_default();
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
            for (root, keys) in by_root {
                let files = {
                    let docs = docs.read().await;
                    project_files_with_open(&root, &docs)
                };
                for (path, src) in &files {
                    if path.extension().and_then(|e| e.to_str()) != Some("mist") {
                        continue;
                    }
                    let Some(dir) = path.parent() else { continue };
                    if !keys.iter().any(|k| imports_path(src, dir, k)) {
                        continue;
                    }
                    let Ok(canon) = path.canonicalize() else { continue };
                    let Ok(uri) = Url::from_file_path(&canon) else { continue };
                    let diagnostics = diagnostics_for(&uri, src);
                    if diag_gen.read().await.get(&gen_key) != Some(&generation) {
                        return;
                    }
                    client.publish_diagnostics(uri, diagnostics, None).await;
                }
            }
            if diag_gen.read().await.get(&gen_key) == Some(&generation) {
                pending.write().await.retain(|p| !snapshot.contains(p));
            }
        });
    }

    async fn project_files(&self, root: &Path) -> Vec<(PathBuf, String)> {
        let docs = self.docs.read().await;
        project_files_with_open(root, &docs)
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["{".into(), ".".into(), "<".into()]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".into(), ",".into()]),
                    ..Default::default()
                }),
                rename_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
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

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let src = match self.docs.read().await.get(&uri) {
            Some(s) => s.clone(),
            None => return Ok(None),
        };
        let actions: Vec<CodeActionOrCommand> = params
            .context
            .diagnostics
            .iter()
            .flat_map(|d| quick_fixes(&src, &uri, d))
            .collect();
        if actions.is_empty() {
            return Ok(None);
        }
        Ok(Some(actions))
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.publish(params.text_document.uri, params.text_document.text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let src = {
            let mut docs = self.docs.write().await;
            let entry = docs.entry(uri.clone()).or_default();
            for change in params.content_changes {
                apply_change(entry, change.range, &change.text);
            }
            entry.clone()
        };
        self.publish_debounced(uri, src).await;
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        let changed: Vec<PathBuf> = params
            .changes
            .into_iter()
            .filter_map(|change| change.uri.to_file_path().ok())
            .filter(|path| {
                matches!(path.extension().and_then(|e| e.to_str()), Some("ts") | Some("mist"))
            })
            .collect();
        if !changed.is_empty() {
            self.rediagnose_watched(changed).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.docs.write().await.remove(&params.text_document.uri);
        self.diag_gen.write().await.remove(&params.text_document.uri);
        self.client.publish_diagnostics(params.text_document.uri, Vec::new(), None).await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let docs = self.docs.read().await;
        let Some(src) = docs.get(&uri) else { return Ok(None) };
        Ok(completions_for(&uri, src, position, &docs).map(CompletionResponse::Array))
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
        let valid = !new_name.is_empty()
            && new_name.chars().next().map(|c| c.is_alphabetic() || c == '_' || c == '$').unwrap_or(false)
            && new_name.chars().all(ident_char);
        if !valid {
            return Err(tower_lsp::jsonrpc::Error::invalid_params("not a valid identifier"));
        }
        if !symbol.local {
            let Some((store_path, _)) = &symbol.def_file else {
                return Err(tower_lsp::jsonrpc::Error::invalid_params(
                    "store module not found on disk — save it first",
                ));
            };
            let Ok(store_canon) = store_path.canonicalize() else {
                return Err(tower_lsp::jsonrpc::Error::invalid_params(
                    "store module not found on disk — save it first",
                ));
            };
            let store_src = std::fs::read_to_string(&store_canon).unwrap_or_default();
            if find_store_decl(&store_src, &new_name).is_some() {
                return Err(tower_lsp::jsonrpc::Error::invalid_params(
                    "name already used in the store module",
                ));
            }
            let Some(root) = doc_dir(&uri).as_deref().and_then(project_src_root) else {
                return Err(tower_lsp::jsonrpc::Error::invalid_params(
                    "no app.mist project root found — cross-file rename needs a project",
                ));
            };
            drop(docs);
            let files = self.project_files(&root).await;
            let changes = store_rename_edits(&store_canon, &word, &new_name, &files);
            if changes.is_empty() {
                return Ok(None);
            }
            if let Some(clash) = mist_decl_collision(&files, &changes, &new_name) {
                return Err(tower_lsp::jsonrpc::Error::invalid_params(format!(
                    "'{}' is already declared in {}",
                    new_name,
                    clash.display()
                )));
            }
            return Ok(Some(WorkspaceEdit { changes: Some(changes), ..Default::default() }));
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

    fn edits_of(a: &CodeActionOrCommand, uri: &Url) -> Vec<TextEdit> {
        let CodeActionOrCommand::CodeAction(a) = a else { panic!("not an action") };
        a.edit.as_ref().unwrap().changes.as_ref().unwrap().get(uri).unwrap().clone()
    }

    fn apply(src: &str, mut edits: Vec<TextEdit>) -> String {
        edits.sort_by_key(|e| std::cmp::Reverse(position_to_byte(src, e.range.start)));
        let mut out = src.to_string();
        for e in edits {
            let s = position_to_byte(src, e.range.start);
            let t = position_to_byte(src, e.range.end);
            out.replace_range(s..t, &e.new_text);
        }
        out
    }

    #[test]
    fn unit_warnings_surface_as_warning_diagnostics() {
        let uri = Url::parse("file:///tmp/pages/x.mist").unwrap();
        let src = "---\nimport { state } from 'mist'\nconst items = state([1])\n---\n<div>{items.value.map(t => (<span>{t}</span>))}</div>\n";
        let ds = diagnostics_for(&uri, src);
        let w = ds.iter().find(|d| d.code == Some(NumberOrString::String("M1008".into()))).expect("M1008 missing");
        assert_eq!(w.severity, Some(DiagnosticSeverity::WARNING));
        assert!(w.range != Range::default(), "range: {:?}", w.range);
    }

    #[test]
    fn m1007_quick_fix_appends_value() {
        let uri = Url::parse("file:///tmp/pages/x.mist").unwrap();
        let src = "---\nimport { state } from 'mist'\nconst count = state(0)\nfunction f() { return count + 1 }\n---\n<span onTap={f}>{count.value}</span>\n";
        let ds = diagnostics_for(&uri, src);
        let d = ds.iter().find(|d| d.code == Some(NumberOrString::String("M1007".into()))).expect("M1007 missing");
        let fixes = quick_fixes(src, &uri, d);
        assert_eq!(fixes.len(), 1);
        let fixed = apply(src, edits_of(&fixes[0], &uri));
        assert!(fixed.contains("return count.value + 1"), "fixed:\n{}", fixed);
    }

    #[test]
    fn m1023_quick_fixes_rename_and_suppress() {
        let uri = Url::parse("file:///tmp/pages/x.mist").unwrap();
        let src = "---\nimport { state } from 'mist'\nexport const config = { navigationBarTitleText: 't' }\nconst n = state(0)\nfunction f() { n.value++ }\n---\n<scroll-view scroll-y onScrolls={f}>{n.value}</scroll-view>\n";
        let ds = diagnostics_for(&uri, src);
        let d = ds.iter().find(|d| d.code == Some(NumberOrString::String("M1023".into()))).expect("M1023 missing");
        let fixes = quick_fixes(src, &uri, d);
        assert_eq!(fixes.len(), 2, "{:?}", fixes.iter().map(|f| match f { CodeActionOrCommand::CodeAction(a) => a.title.clone(), _ => String::new() }).collect::<Vec<_>>());
        let renamed = apply(src, edits_of(&fixes[0], &uri));
        assert!(renamed.contains("onScroll={f}"), "renamed:\n{}", renamed);
        let suppressed = apply(src, edits_of(&fixes[1], &uri));
        assert!(suppressed.contains("export const config = { customAttrs: ['onScrolls'], navigationBarTitleText: 't' }"), "suppressed:\n{}", suppressed);
    }

    #[test]
    fn m1027_quick_fix_sets_min_lib_version() {
        let uri = Url::parse("file:///tmp/pages/x.mist").unwrap();
        let src = "---\nimport { state } from 'mist'\nexport const config = { minLibVersion: '2.0.0' }\nconst n = state(0)\n---\n<scroll-view refresher-enabled>{n.value}</scroll-view>\n";
        let ds = diagnostics_for(&uri, src);
        let d = ds.iter().find(|d| d.code == Some(NumberOrString::String("M1027".into()))).expect("M1027 missing");
        let fixes = quick_fixes(src, &uri, d);
        assert_eq!(fixes.len(), 1);
        let fixed = apply(src, edits_of(&fixes[0], &uri));
        assert!(fixed.contains("minLibVersion: '2.10.1'"), "fixed:\n{}", fixed);
    }

    #[test]
    fn m1026_quick_fix_wraps_raw_and_extends_import() {
        let uri = Url::parse("file:///tmp/pages/x.mist").unwrap();
        let src = "---\nimport { state } from 'mist'\nimport dayjs from 'dayjs'\nconst when = state({ ts: 0 })\nfunction f() { return dayjs(when.value) }\n---\n<span>{f()}</span>\n";
        let ds = diagnostics_for(&uri, src);
        let d = ds.iter().find(|d| d.code == Some(NumberOrString::String("M1026".into()))).expect("M1026 missing");
        let fixes = quick_fixes(src, &uri, d);
        assert_eq!(fixes.len(), 1);
        let fixed = apply(src, edits_of(&fixes[0], &uri));
        assert!(fixed.contains("dayjs(raw(when.value))"), "fixed:\n{}", fixed);
        assert!(fixed.contains("import { raw, state } from 'mist'"), "fixed:\n{}", fixed);
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
    fn template_context_classifies_cursor_positions() {
        let src = "---\n---\n<scroll-view scroll-y onTap={f}>\n  <span>{open.value}</span>\n</scroll-view>\n";
        let at = |line: u32, character: u32| template_context(src, Position { line, character });
        assert!(matches!(at(2, 8), Some(TemplateCtx::TagName)), "mid tag name");
        match at(2, 13) {
            Some(TemplateCtx::AttrPosition { tag }) => assert_eq!(tag, "scroll-view"),
            other => panic!("expected attr position, got {:?}", other.is_some()),
        }
        match at(2, 21) {
            Some(TemplateCtx::AttrPosition { tag }) => assert_eq!(tag, "scroll-view"),
            other => panic!("expected attr position after attr, got {:?}", other.is_some()),
        }
        assert!(at(2, 30).is_none(), "inside {{expr}} must fall back to symbols");
        assert!(at(3, 10).is_none(), "inside text content");
        assert!(at(3, 12).is_none(), "inside expr braces");
        assert!(
            matches!(at(4, 4), Some(TemplateCtx::ClosingTag)),
            "closing tag must suppress completions"
        );
    }

    #[test]
    fn template_context_survives_arrow_handlers_and_quoted_gt() {
        let src = "---\n---\n<span onTap={() => f(1)} data-x=\"a>b\" >\n";
        match template_context(src, Position { line: 2, character: 25 }) {
            Some(TemplateCtx::AttrPosition { tag }) => assert_eq!(tag, "span"),
            other => panic!("arrow > must not close the tag, got {:?}", other.is_some()),
        }
        match template_context(src, Position { line: 2, character: 38 }) {
            Some(TemplateCtx::AttrPosition { tag }) => assert_eq!(tag, "span"),
            other => panic!("quoted > must not close the tag, got {:?}", other.is_some()),
        }
        let closed = template_context(src, Position { line: 2, character: 40 });
        assert!(closed.is_none() || matches!(closed, Some(TemplateCtx::AttrPosition { .. })));
        let after = "---\n---\n<view>text \n";
        assert!(
            template_context(after, Position { line: 2, character: 11 }).is_none(),
            "cursor after a closed tag must not complete attrs"
        );
        let closing = "---\n---\n<image src=\"x\" /\n";
        assert!(
            template_context(closing, Position { line: 2, character: 16 }).is_none(),
            "mid-typing /> must not offer attributes"
        );
    }

    #[test]
    fn open_or_disk_never_matches_unrelated_buffers() {
        let mut docs = HashMap::new();
        docs.insert(
            Url::parse("file:///nonexistent-open-doc/a.mist").unwrap(),
            "unsaved buffer".to_string(),
        );
        assert_eq!(
            open_or_disk(&docs, Path::new("/nonexistent-target/b.mist")),
            None,
            "a missing target must not resolve to an unrelated open buffer"
        );
        let dir = std::env::temp_dir().join("mist-lsp-open-or-disk");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("c.mist"), "on disk").unwrap();
        let canon = dir.join("c.mist").canonicalize().unwrap();
        docs.insert(Url::from_file_path(&canon).unwrap(), "open version".to_string());
        assert_eq!(
            open_or_disk(&docs, &dir.join("c.mist")).as_deref(),
            Some("open version"),
            "open buffers must win over disk"
        );
    }

    #[test]
    fn template_context_skips_quoted_values() {
        let src = "---\n---\n<image src=\"/a b.png\" mode=\"aspectFill\" />\n";
        let inside_quote = template_context(src, Position { line: 2, character: 15 });
        assert!(inside_quote.is_none(), "inside a quoted value");
        match template_context(src, Position { line: 2, character: 22 }) {
            Some(TemplateCtx::AttrPosition { tag }) => assert_eq!(tag, "image"),
            other => panic!("expected attr position between attrs, got {:?}", other.is_some()),
        }
    }

    #[test]
    fn native_attr_completions_cover_meta_and_events() {
        let labels: Vec<String> =
            native_attr_completions("scroll-view").into_iter().map(|c| c.label).collect();
        assert!(labels.contains(&"scroll-y".to_string()), "labels: {:?}", labels);
        assert!(labels.contains(&"refresher-enabled".to_string()), "labels: {:?}", labels);
        assert!(labels.contains(&"onScrollToLower".to_string()), "labels: {:?}", labels);
        assert!(labels.contains(&"onTap".to_string()), "labels: {:?}", labels);
        assert!(labels.contains(&"class".to_string()), "labels: {:?}", labels);
        assert!(!labels.contains(&"value:bind".to_string()), "labels: {:?}", labels);

        let div: Vec<String> =
            native_attr_completions("div").into_iter().map(|c| c.label).collect();
        assert!(div.contains(&"hover-class".to_string()), "aliases map to view: {:?}", div);
        let input: Vec<String> =
            native_attr_completions("input").into_iter().map(|c| c.label).collect();
        assert!(input.contains(&"value:bind".to_string()), "labels: {:?}", input);
        assert!(input.contains(&"placeholder-class".to_string()), "labels: {:?}", input);
    }

    #[test]
    fn apply_change_replaces_ranges_incrementally() {
        let mut src = "const a = 1\nconst b = 2\n".to_string();
        apply_change(
            &mut src,
            Some(Range {
                start: Position { line: 1, character: 6 },
                end: Position { line: 1, character: 7 },
            }),
            "count",
        );
        assert_eq!(src, "const a = 1\nconst count = 2\n");
        apply_change(
            &mut src,
            Some(Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 0, character: 0 },
            }),
            "x",
        );
        assert_eq!(src, "xconst a = 1\nconst count = 2\n");
        apply_change(&mut src, None, "reset");
        assert_eq!(src, "reset");
    }

    #[test]
    fn apply_change_handles_utf16_columns() {
        let mut src = "let 名字 = old\n".to_string();
        apply_change(
            &mut src,
            Some(Range {
                start: Position { line: 0, character: 9 },
                end: Position { line: 0, character: 12 },
            }),
            "new",
        );
        assert_eq!(src, "let 名字 = new\n");
    }

    #[test]
    fn position_to_byte_clamps_past_end() {
        let src = "ab\ncd";
        assert_eq!(position_to_byte(src, Position { line: 5, character: 0 }), src.len());
        assert_eq!(position_to_byte(src, Position { line: 1, character: 99 }), src.len());
    }

    #[test]
    fn store_rename_edits_span_project_files() {
        let dir = std::env::temp_dir().join("mist-lsp-rename-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        std::fs::create_dir_all(dir.join("stores")).unwrap();
        let store = "import { store } from 'mist'\nexport const cart = store({ n: 0 })\nexport function track() { cart.value.n++ }\n";
        let page = "---\nimport { cart, track } from '../stores/stats.ts'\nfunction go() { track() }\n---\n<span onTap={go}>{cart.value.n}</span>\n";
        let other = "---\nimport { state } from 'mist'\nconst track = state(0)\n---\n<span>{track.value}</span>\n";
        std::fs::write(dir.join("stores/stats.ts"), store).unwrap();
        std::fs::write(dir.join("pages/index.mist"), page).unwrap();
        std::fs::write(dir.join("pages/other.mist"), other).unwrap();
        let files = vec![
            (dir.join("stores/stats.ts"), store.to_string()),
            (dir.join("pages/index.mist"), page.to_string()),
            (dir.join("pages/other.mist"), other.to_string()),
        ];
        let store_canon = dir.join("stores/stats.ts").canonicalize().unwrap();
        let changes = store_rename_edits(&store_canon, "track", "record", &files);
        assert_eq!(changes.len(), 2, "changes: {:?}", changes.keys().collect::<Vec<_>>());
        let store_uri = Url::from_file_path(&store_canon).unwrap();
        assert_eq!(changes[&store_uri].len(), 1);
        let page_uri =
            Url::from_file_path(dir.join("pages/index.mist").canonicalize().unwrap()).unwrap();
        assert_eq!(changes[&page_uri].len(), 2);
        assert!(changes
            .keys()
            .all(|u| !u.path().ends_with("other.mist")), "unrelated local `track` must not be renamed");
    }

    #[test]
    fn mist_decl_collision_flags_affected_pages_only() {
        let dir = std::env::temp_dir().join("mist-lsp-collision-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        let page = "---\nimport { track } from '../stores/stats.ts'\nconst record = 1\n---\n<span>{track()}</span>\n";
        std::fs::write(dir.join("pages/index.mist"), page).unwrap();
        let canon = dir.join("pages/index.mist").canonicalize().unwrap();
        let uri = Url::from_file_path(&canon).unwrap();
        let files = vec![(dir.join("pages/index.mist"), page.to_string())];
        let mut changes = HashMap::new();
        changes.insert(uri, Vec::new());
        assert!(mist_decl_collision(&files, &changes, "record").is_some());
        assert!(mist_decl_collision(&files, &changes, "unused").is_none());
        assert!(mist_decl_collision(&files, &HashMap::new(), "record").is_none());
    }

    #[test]
    fn store_importers_finds_only_importing_pages() {
        let dir = std::env::temp_dir().join("mist-lsp-ws-diag-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        std::fs::create_dir_all(dir.join("stores")).unwrap();
        std::fs::write(dir.join("stores/cart.ts"), "export function add() {}\n").unwrap();
        let importer = "---\nimport { add } from '../stores/cart.ts'\n---\n<span onTap={add}>x</span>\n";
        let bystander = "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n";
        std::fs::write(dir.join("pages/a.mist"), importer).unwrap();
        std::fs::write(dir.join("pages/b.mist"), bystander).unwrap();
        let canon = dir.join("stores/cart.ts").canonicalize().unwrap();
        assert!(imports_path(importer, &dir.join("pages"), &canon), "importer must match");
        assert!(!imports_path(bystander, &dir.join("pages"), &canon), "bystander must not");
        let deleted = dir.join("stores/gone.ts");
        let deleted_key = resolved_key(&deleted).unwrap();
        let ghost = "---\nimport { x } from '../stores/gone.ts'\n---\n<span onTap={x}>y</span>\n";
        assert!(
            imports_path(ghost, &dir.join("pages"), &deleted_key),
            "deleted targets must still match via resolved_key"
        );
    }

    #[test]
    fn imports_store_requires_matching_path_and_name() {
        let dir = std::env::temp_dir().join("mist-lsp-imports-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("stores")).unwrap();
        std::fs::write(dir.join("stores/a.ts"), "export function f() {}\n").unwrap();
        std::fs::write(dir.join("stores/b.ts"), "export function f() {}\n").unwrap();
        let canon_a = dir.join("stores/a.ts").canonicalize().unwrap();
        let src = "import { f } from './stores/a.ts'\n";
        assert!(imports_store(src, &dir, &canon_a, "f"));
        assert!(!imports_store(src, &dir, &canon_a, "g"));
        let src_b = "import { f } from './stores/b.ts'\n";
        assert!(!imports_store(src_b, &dir, &canon_a, "f"));
        assert!(!imports_store("import { f } from 'mist'\n", &dir, &canon_a, "f"));
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
    let (service, socket) = LspService::new(|client| Backend {
        client,
        docs: Arc::new(RwLock::new(HashMap::new())),
        diag_gen: Arc::new(RwLock::new(HashMap::new())),
        gen_counter: AtomicU64::new(0),
        pending_watched: Arc::new(RwLock::new(Vec::new())),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
