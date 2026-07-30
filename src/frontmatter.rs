use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_ast::visit::walk;
use oxc_ast::Visit;
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType, Span};
use regex::Regex;

use crate::wxml::Handler;

pub struct Analysis {
    pub states: Vec<StateDecl>,
    pub deriveds: Vec<DerivedDecl>,
    pub methods: Vec<Method>,
    pub lifecycles: Vec<Lifecycle>,
    pub config: Option<String>,
    pub plain_stmts: Vec<String>,
    pub imports: Vec<MistImport>,
    pub store_imports: Vec<StoreImport>,
    pub data_props: Vec<PropDecl>,
    pub callback_props: Vec<String>,
}

pub struct MistImport {
    pub local: String,
    pub path: String,
}

#[derive(Clone, Debug, Default)]
pub struct StoreModuleInfo {
    pub stores: Vec<String>,
    pub fns: Vec<String>,
}

pub struct StoreImport {
    pub path: String,
    pub alias: String,
    pub stores: Vec<String>,
    pub fns: Vec<String>,
    pub require_path: String,
}

pub struct PropDecl {
    pub name: String,
    pub default: Option<String>,
}

pub struct StateDecl {
    pub name: String,
    pub init: String,
    pub bound: bool,
}

pub struct DerivedDecl {
    pub name: String,
    pub arrow: String,
}

pub struct Method {
    pub name: String,
    pub params: String,
    pub body: String,
    pub is_async: bool,
}

pub struct Lifecycle {
    pub hook: String,
    pub params: String,
    pub body: String,
    pub is_async: bool,
}

const LIFECYCLE_HOOKS: &[&str] =
    &["onLoad", "onShow", "onReady", "onHide", "onUnload", "onAttach", "onDetach", "onLaunch"];

struct Edit {
    start: u32,
    end: u32,
    text: String,
}

pub fn analyze(src: &str) -> Result<Analysis, String> {
    analyze_with_stores_bound(src, &|_| None, 1, None)
}

pub fn line_col(src: &str, byte: usize, line_offset: usize) -> (usize, usize) {
    let byte = byte.min(src.len());
    let line = src[..byte].matches('\n').count() + line_offset;
    let col = byte - src[..byte].rfind('\n').map(|i| i + 1).unwrap_or(0) + 1;
    (line, col)
}

pub fn analyze_with_stores(
    src: &str,
    resolve: &dyn Fn(&str) -> Option<StoreModuleInfo>,
    line_offset: usize,
) -> Result<Analysis, String> {
    analyze_with_stores_bound(src, resolve, line_offset, None)
}

pub fn analyze_with_stores_bound(
    src: &str,
    resolve: &dyn Fn(&str) -> Option<StoreModuleInfo>,
    line_offset: usize,
    template: Option<&str>,
) -> Result<Analysis, String> {
    let allocator = Allocator::default();
    let source_type = SourceType::default().with_typescript(true);
    let ret = Parser::new(&allocator, src, source_type).parse();
    if !ret.errors.is_empty() {
        let msgs: Vec<String> = ret.errors.iter().map(|e| e.to_string()).collect();
        return Err(format!("frontmatter parse errors: {}", msgs.join("; ")));
    }
    let program = ret.program;

    let mut states: Vec<StateDecl> = Vec::new();
    let mut deriveds: Vec<DerivedDecl> = Vec::new();
    let mut raw_methods: Vec<(String, Span, Span, bool)> = Vec::new(); // name, params-ish span gap, body span
    let mut raw_lifecycles: Vec<(String, Span, Span, bool, bool)> = Vec::new();
    let mut config: Option<String> = None;
    let mut plain_stmts: Vec<Span> = Vec::new();
    let mut imports: Vec<MistImport> = Vec::new();
    let mut data_props: Vec<PropDecl> = Vec::new();
    let mut callback_props: Vec<String> = Vec::new();
    let mut store_imports: Vec<StoreImport> = Vec::new();

    for stmt in &program.body {
        match stmt {
            Statement::ImportDeclaration(import) => {
                let path = import.source.value.to_string();
                if path.ends_with(".mist") {
                    let local = import
                        .specifiers
                        .as_ref()
                        .and_then(|specs| {
                            specs.iter().find_map(|s| match s {
                                ImportDeclarationSpecifier::ImportDefaultSpecifier(d) => {
                                    Some(d.local.name.to_string())
                                }
                                _ => None,
                            })
                        })
                        .ok_or_else(|| format!("component import '{}' needs a default import", path))?;
                    imports.push(MistImport { local, path });
                } else if path.starts_with("./") || path.starts_with("../") {
                    let Some(info) = resolve(&path) else {
                        return Err(format!(
                            "cannot resolve import '{}' — store modules require a project build",
                            path
                        ));
                    };
                    let mut stores = Vec::new();
                    let mut fns = Vec::new();
                    if let Some(specs) = import.specifiers.as_ref() {
                        for s in specs.iter() {
                            let ImportDeclarationSpecifier::ImportSpecifier(named) = s else {
                                return Err(format!("store module '{}' supports named imports only", path));
                            };
                            let name = named.local.name.to_string();
                            if info.stores.contains(&name) {
                                stores.push(name);
                            } else if info.fns.contains(&name) {
                                fns.push(name);
                            } else {
                                return Err(format!("'{}' is not exported by store module '{}'", name, path));
                            }
                        }
                    }
                    let alias = format!("__S{}", store_imports.len());
                    store_imports.push(StoreImport {
                        path,
                        alias,
                        stores,
                        fns,
                        require_path: String::new(),
                    });
                }
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(Declaration::VariableDeclaration(var)) = &export.declaration {
                    for decl in &var.declarations {
                        if binding_name(&decl.id).as_deref() == Some("config") {
                            if let Some(init) = &decl.init {
                                config = Some(text(src, init.span()).to_string());
                            }
                        }
                    }
                }
            }
            Statement::VariableDeclaration(var) => {
                let mut handled = false;
                for decl in &var.declarations {
                    if let Some(Expression::CallExpression(call)) = decl.init.as_ref().map(unparenthesize) {
                        if callee_name(call) == Some("props") {
                            let BindingPatternKind::ObjectPattern(pat) = &decl.id.kind else {
                                return Err("props() must be destructured: const { ... } = props()".to_string());
                            };
                            let defaults = props_defaults(src, call);
                            for prop in &pat.properties {
                                let Some(key) = prop.key.static_name() else {
                                    return Err("props() destructuring must use plain identifiers".to_string());
                                };
                                let key = key.to_string();
                                if is_callback_prop(&key) {
                                    callback_props.push(key);
                                } else {
                                    let default = defaults
                                        .iter()
                                        .find(|(k, _)| *k == key)
                                        .map(|(_, v)| v.clone());
                                    data_props.push(PropDecl { name: key, default });
                                }
                            }
                            handled = true;
                            continue;
                        }
                    }
                    let Some(name) = binding_name(&decl.id) else { continue };
                    if let Some(Expression::CallExpression(call)) = decl.init.as_ref().map(unparenthesize) {
                        if let Some(callee) = callee_name(call) {
                            match callee {
                                "state" => {
                                    let init = call
                                        .arguments
                                        .first()
                                        .map(|a| text(src, a.span()).to_string())
                                        .unwrap_or_else(|| "null".to_string());
                                    states.push(StateDecl { name, init, bound: true });
                                    handled = true;
                                    continue;
                                }
                                "derived" => {
                                    let arrow = call
                                        .arguments
                                        .first()
                                        .map(|a| text(src, a.span()).to_string())
                                        .ok_or("derived() requires a function argument")?;
                                    deriveds.push(DerivedDecl { name, arrow });
                                    handled = true;
                                    continue;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                if !handled {
                    plain_stmts.push(var.span);
                }
            }
            Statement::FunctionDeclaration(func) => {
                let name = func
                    .id
                    .as_ref()
                    .map(|id| id.name.to_string())
                    .ok_or("anonymous top-level function")?;
                let body = func.body.as_ref().ok_or("function without body")?;
                let params_gap = Span::new(func.id.as_ref().unwrap().span.end, body.span.start);
                raw_methods.push((name, params_gap, body.span, func.r#async));
            }
            Statement::ExpressionStatement(expr_stmt) => {
                if let Expression::CallExpression(call) = unparenthesize(&expr_stmt.expression) {
                    if let Some(callee) = callee_name(call) {
                        if LIFECYCLE_HOOKS.contains(&callee) {
                            let arg = call.arguments.first().ok_or("lifecycle hook requires a callback")?;
                            let Some(Expression::ArrowFunctionExpression(arrow)) = arg.as_expression().map(unparenthesize)
                            else {
                                return Err(format!("{} expects an arrow function", callee));
                            };
                            raw_lifecycles.push((
                                callee.to_string(),
                                arrow.params.span,
                                arrow.body.span,
                                arrow.r#async,
                                arrow.expression,
                            ));
                            continue;
                        }
                    }
                }
                plain_stmts.push(expr_stmt.span);
            }
            other => {
                plain_stmts.push(other.span());
            }
        }
    }

    if let Some(blob) = template {
        for s in states.iter_mut() {
            s.bound = blob.contains(&format!("{}.value", s.name))
                || blob.contains(&format!("value:bind={{{}}}", s.name))
                || blob.contains(&format!("value:bind={{{}.value}}", s.name));
        }
    }
    let state_names: Vec<String> = states.iter().map(|s| s.name.clone()).collect();
    let bound_names: Vec<String> =
        states.iter().filter(|s| s.bound).map(|s| s.name.clone()).collect();
    let unbound_names: Vec<String> =
        states.iter().filter(|s| !s.bound).map(|s| s.name.clone()).collect();
    let derived_names: Vec<String> = deriveds.iter().map(|d| d.name.clone()).collect();
    let mut reactive: Vec<String> = bound_names.clone();
    reactive.extend(derived_names.clone());
    let _ = &state_names;

    {
        let mut ns: std::collections::BTreeMap<String, &str> = std::collections::BTreeMap::new();
        let mut check = |name: &str, kind: &'static str| -> Result<(), String> {
            match ns.insert(name.to_string(), kind) {
                Some(prev) if prev == kind => Err(format!("M1005: {} '{}' is declared twice", kind, name)),
                Some(prev) => Err(format!("M1005: '{}' is declared as both {} and {}", name, prev, kind)),
                None => Ok(()),
            }
        };
        for s in &states {
            check(&s.name, "state")?;
        }
        for d in &deriveds {
            check(&d.name, "derived")?;
        }
        for p in &data_props {
            check(&p.name, "prop")?;
        }
        for c in &callback_props {
            check(c, "callback prop")?;
        }
        for (n, ..) in &raw_methods {
            check(n, "method")?;
        }
        for si in &store_imports {
            for s in &si.stores {
                check(s, "store import")?;
            }
            for f in &si.fns {
                check(f, "store function")?;
            }
        }
    }

    let store_accessors: std::collections::BTreeMap<String, String> = store_imports
        .iter()
        .flat_map(|si| si.stores.iter().map(|s| (s.clone(), format!("{}.{}", si.alias, s))))
        .collect();

    let mut collector = MutationCollector {
        src,
        states: bound_names.clone(),
        unbound: unbound_names.clone(),
        stores: store_accessors.clone(),
        edits: Vec::new(),
        errors: Vec::new(),
        line_offset,
    };
    collector.visit_program(&program);
    if !collector.errors.is_empty() {
        return Err(collector.errors.join("; "));
    }
    let edits = collector.edits;

    let mut method_names: Vec<String> = raw_methods.iter().map(|(n, ..)| n.clone()).collect();
    method_names.extend(callback_props.iter().cloned());
    method_names.extend(store_imports.iter().flat_map(|si| si.fns.iter().cloned()));
    let rewriter = Rewriter::new(&reactive, &method_names, &store_accessors, &unbound_names)?;

    let methods = raw_methods
        .into_iter()
        .map(|(name, params_gap, body_span, is_async)| Method {
            name,
            params: text(src, params_gap).trim().to_string(),
            body: rewriter.transform(src, body_span, &edits),
            is_async,
        })
        .collect();

    let lifecycles = raw_lifecycles
        .into_iter()
        .map(|(hook, params_span, body_span, is_async, expr_body)| {
            let params = text(src, params_span).trim().to_string();
            let params = if params.starts_with('(') { params } else { format!("({})", params) };
            let body = rewriter.transform(src, body_span, &edits);
            let body = if expr_body { format!("{{ {} }}", body) } else { body };
            Lifecycle { hook, params, body, is_async }
        })
        .collect();

    let deriveds = deriveds
        .into_iter()
        .map(|d| DerivedDecl {
            arrow: rewriter.rewrite_reads(&d.arrow),
            name: d.name,
        })
        .collect();

    let plain_stmts = plain_stmts
        .into_iter()
        .map(|span| text(src, span).to_string())
        .collect();

    Ok(Analysis {
        states,
        deriveds,
        methods,
        lifecycles,
        config,
        plain_stmts,
        imports,
        store_imports,
        data_props,
        callback_props,
    })
}

fn props_defaults(src: &str, call: &CallExpression) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(arg) = call.arguments.first().and_then(|a| a.as_expression()) else {
        return out;
    };
    if let Expression::ObjectExpression(obj) = unparenthesize(arg) {
        for prop in &obj.properties {
            if let ObjectPropertyKind::ObjectProperty(p) = prop {
                if let Some(key) = p.key.static_name() {
                    out.push((key.to_string(), text(src, p.value.span()).to_string()));
                }
            }
        }
    }
    out
}

fn is_callback_prop(name: &str) -> bool {
    name.strip_prefix("on")
        .and_then(|r| r.chars().next())
        .is_some_and(|c| c.is_uppercase())
}

pub fn event_name(callback_prop: &str) -> String {
    let rest = callback_prop.strip_prefix("on").unwrap_or(callback_prop);
    let mut chars = rest.chars();
    match chars.next() {
        Some(c) => format!("{}{}", c.to_lowercase(), chars.as_str()),
        None => String::new(),
    }
}

pub fn emit_js(
    analysis: &Analysis,
    handlers: &[Handler],
    derived_keys: &[Option<String>],
    is_page: bool,
    multiple_slots: bool,
    rt_require: &str,
    vbinds: &[String],
) -> String {
    let mut out = String::new();
    out.push_str("// generated by mistc — do not edit\n");
    out.push_str(&format!("const rt = require('{}');\n", rt_require));
    for si in &analysis.store_imports {
        out.push_str(&format!("const {} = require('{}');\n", si.alias, si.require_path));
    }
    for stmt in &analysis.plain_stmts {
        out.push_str(stmt);
        out.push('\n');
    }

    let field_init: String = analysis
        .states
        .iter()
        .filter(|s| !s.bound)
        .map(|s| format!("this._{} = {};\n    ", s.name, s.init))
        .collect();

    let bind_pairs: Vec<String> = analysis
        .store_imports
        .iter()
        .flat_map(|si| si.stores.iter().map(|s| format!("[{}.{}, '{}']", si.alias, s, s)))
        .collect();
    let bind_stmt = if bind_pairs.is_empty() {
        String::new()
    } else {
        format!("rt.bindStores(this, [{}]);\n    ", bind_pairs.join(", "))
    };
    let bind_stmt = format!("{}{}", field_init, bind_stmt);
    let unbind_stmt = if bind_pairs.is_empty() { "" } else { "rt.unbindStores(this);\n    " };

    let pad = if is_page { "  " } else { "    " };
    let mut body = String::new();

    for si in &analysis.store_imports {
        for f in &si.fns {
            body.push_str(&format!(
                "{}{}(...args) {{\n{}  return {}.{}(...args);\n{}}},\n",
                pad, f, pad, si.alias, f, pad
            ));
        }
    }

    body.push_str(&format!("{}__derive() {{\n{}  const __o = {{}};\n", pad, pad));
    for (i, d) in analysis.deriveds.iter().enumerate() {
        let key = match derived_keys.get(i).and_then(|k| k.as_deref()) {
            Some(k) => format!("'{}'", k),
            None => "null".to_string(),
        };
        body.push_str(&format!("{}  rt.derive(this, __o, '{}', {}, {});\n", pad, d.name, key, d.arrow));
    }
    body.push_str(&format!("{}  return __o;\n{}}},\n", pad, pad));

    body.push_str(&format!("{}__set(path, value) {{\n{}  rt.set(this, path, value);\n{}}},\n", pad, pad, pad));

    for m in &analysis.methods {
        let prefix = if m.is_async { "async " } else { "" };
        body.push_str(&format!("{}{}{}{} {},\n", pad, prefix, m.name, m.params, m.body));
    }

    for cb in &analysis.callback_props {
        body.push_str(&format!(
            "{}{}(...args) {{\n{}  this.triggerEvent('{}', {{ args }});\n{}}},\n",
            pad, cb, pad, event_name(cb), pad
        ));
    }

    for vb in vbinds {
        body.push_str(&format!(
            "{}__vb_{}(e) {{\n{}  this.data.{} = e.detail.value;\n{}  rt.touch(this);\n{}}},\n",
            pad, vb, pad, vb, pad, pad
        ));
    }

    for h in handlers {
        if h.from_detail {
            body.push_str(&format!(
                "{}{}(e) {{\n{}  const a = (e.detail && e.detail.args) || [];\n{}  this.{}(...a);\n{}}},\n",
                pad, h.name, pad, pad, h.target, pad
            ));
        } else {
            let args: Vec<String> =
                (0..h.args.len()).map(|i| format!("e.currentTarget.dataset.a{}", i)).collect();
            body.push_str(&format!(
                "{}{}(e) {{\n{}  this.{}({});\n{}}},\n",
                pad, h.name, pad, h.target, args.join(", "), pad
            ));
        }
    }

    if is_page {
        out.push_str("Page({\n");
        emit_data(&mut out, analysis, "  ");
        out.push_str(&body);

        let mut has_on_load = false;
        let mut has_on_unload = false;
        for l in &analysis.lifecycles {
            let prefix = if l.is_async { "async " } else { "" };
            if l.hook == "onLoad" {
                has_on_load = true;
                let inner = inner_of_block(&l.body);
                out.push_str(&format!(
                    "  {}onLoad{} {{\n    {}rt.init(this);\n{}\n  }},\n",
                    prefix, l.params, bind_stmt, inner
                ));
            } else if l.hook == "onUnload" && !bind_pairs.is_empty() {
                has_on_unload = true;
                let inner = inner_of_block(&l.body);
                out.push_str(&format!(
                    "  {}onUnload{} {{\n    {}{}\n  }},\n",
                    prefix, l.params, unbind_stmt, inner
                ));
            } else {
                out.push_str(&format!("  {}{}{} {},\n", prefix, l.hook, l.params, l.body));
            }
        }
        if !has_on_load {
            out.push_str(&format!("  onLoad() {{\n    {}rt.init(this);\n  }},\n", bind_stmt));
        }
        if !has_on_unload && !bind_pairs.is_empty() {
            out.push_str("  onUnload() {\n    rt.unbindStores(this);\n  },\n");
        }
        out.push_str("});\n");
        return out;
    }

    out.push_str("Component({\n");
    if multiple_slots {
        out.push_str("  options: { multipleSlots: true },\n");
    }
    out.push_str("  properties: {\n");
    for p in &analysis.data_props {
        match &p.default {
            Some(d) => out.push_str(&format!("    {}: {{ type: null, value: {} }},\n", p.name, d)),
            None => out.push_str(&format!("    {}: {{ type: null }},\n", p.name)),
        }
    }
    out.push_str("  },\n");
    emit_data(&mut out, analysis, "  ");
    out.push_str("  methods: {\n");
    out.push_str(&body);
    out.push_str("  },\n");

    out.push_str("  lifetimes: {\n");
    let mut has_attached = false;
    let mut has_detached = false;
    for l in &analysis.lifecycles {
        let prefix = if l.is_async { "async " } else { "" };
        let hook = match l.hook.as_str() {
            "onAttach" => "attached",
            "onDetach" => "detached",
            "onReady" => "ready",
            other => other,
        };
        if hook == "attached" {
            has_attached = true;
            let inner = inner_of_block(&l.body);
            out.push_str(&format!(
                "    {}attached{} {{\n      {}rt.init(this);\n{}\n    }},\n",
                prefix, l.params, bind_stmt, inner
            ));
        } else if hook == "detached" && !bind_pairs.is_empty() {
            has_detached = true;
            let inner = inner_of_block(&l.body);
            out.push_str(&format!(
                "    {}detached{} {{\n      {}{}\n    }},\n",
                prefix, l.params, unbind_stmt, inner
            ));
        } else {
            out.push_str(&format!("    {}{}{} {},\n", prefix, hook, l.params, l.body));
        }
    }
    if !has_attached {
        out.push_str(&format!("    attached() {{\n      {}rt.init(this);\n    }},\n", bind_stmt));
    }
    if !has_detached && !bind_pairs.is_empty() {
        out.push_str("    detached() {\n      rt.unbindStores(this);\n    },\n");
    }
    out.push_str("  },\n");
    out.push_str("});\n");
    out
}

pub fn compile_store_module(src: &str, rt_require: &str) -> Result<(String, StoreModuleInfo), String> {
    let allocator = Allocator::default();
    let source_type = SourceType::default().with_typescript(true);
    let ret = Parser::new(&allocator, src, source_type).parse();
    if !ret.errors.is_empty() {
        let msgs: Vec<String> = ret.errors.iter().map(|e| e.to_string()).collect();
        return Err(format!("store parse errors: {}", msgs.join("; ")));
    }
    let program = ret.program;

    let mut info = StoreModuleInfo::default();
    for stmt in &program.body {
        if let Statement::ExportNamedDeclaration(export) = stmt {
            if let Some(Declaration::VariableDeclaration(var)) = &export.declaration {
                for decl in &var.declarations {
                    let Some(name) = binding_name(&decl.id) else { continue };
                    if let Some(Expression::CallExpression(call)) = decl.init.as_ref().map(unparenthesize) {
                        if callee_name(call) == Some("store") {
                            info.stores.push(name);
                        } else if callee_name(call) == Some("derived") {
                            return Err("derived() in store modules is not supported yet".to_string());
                        }
                    }
                }
            }
            if let Some(Declaration::FunctionDeclaration(func)) = &export.declaration {
                if let Some(id) = &func.id {
                    info.fns.push(id.name.to_string());
                }
            }
        }
    }

    let accessors: std::collections::BTreeMap<String, String> =
        info.stores.iter().map(|s| (s.clone(), s.clone())).collect();
    let mut collector = MutationCollector {
        src,
        states: Vec::new(),
        unbound: Vec::new(),
        stores: accessors,
        edits: Vec::new(),
        errors: Vec::new(),
        line_offset: 1,
    };
    collector.visit_program(&program);
    if !collector.errors.is_empty() {
        return Err(collector.errors.join("; "));
    }
    let edits = collector.edits;

    let apply = |span: Span| -> String {
        let start = span.start;
        let mut slice = text(src, span).to_string();
        let mut local: Vec<&Edit> = edits.iter().filter(|e| e.start >= start && e.end <= span.end).collect();
        local.sort_by(|a, b| b.start.cmp(&a.start));
        for e in local {
            slice.replace_range((e.start - start) as usize..(e.end - start) as usize, &e.text);
        }
        slice
    };

    let mut out = String::new();
    out.push_str("// generated by mistc — do not edit\n");
    out.push_str(&format!("const rt = require('{}');\n", rt_require));
    for stmt in &program.body {
        match stmt {
            Statement::ImportDeclaration(import) => {
                if import.source.value != "mist" {
                    return Err(format!(
                        "store modules can only import from 'mist' (found '{}')",
                        import.source.value
                    ));
                }
            }
            Statement::ExportNamedDeclaration(export) => match &export.declaration {
                Some(Declaration::VariableDeclaration(var)) => {
                    for decl in &var.declarations {
                        let Some(name) = binding_name(&decl.id) else {
                            return Err("store exports must be simple bindings".to_string());
                        };
                        let Some(init) = &decl.init else {
                            return Err(format!("store export '{}' needs an initializer", name));
                        };
                        if info.stores.contains(&name) {
                            let arg = match unparenthesize(init) {
                                Expression::CallExpression(call) => call
                                    .arguments
                                    .first()
                                    .map(|a| text(src, a.span()).to_string())
                                    .unwrap_or_else(|| "null".to_string()),
                                _ => "null".to_string(),
                            };
                            out.push_str(&format!("const {} = rt.store({});\n", name, arg));
                        } else if matches!(
                            unparenthesize(init),
                            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
                        ) {
                            out.push_str(&format!("const {} = {};\n", name, apply(init.span())));
                            info.fns.push(name);
                        } else {
                            return Err(format!(
                                "store export '{}' must be a store() or a function — plain values cannot cross the page boundary",
                                name
                            ));
                        }
                    }
                }
                Some(Declaration::FunctionDeclaration(func)) => {
                    out.push_str(&apply(func.span));
                    out.push('\n');
                }
                _ => return Err("unsupported export in store module".to_string()),
            },
            other => {
                out.push_str(&apply(other.span()));
                out.push('\n');
            }
        }
    }
    let mut exports: Vec<String> = info.stores.clone();
    exports.extend(info.fns.iter().cloned());
    out.push_str(&format!("module.exports = {{ {} }};\n", exports.join(", ")));
    Ok((out, info))
}

pub fn store_module_info(src: &str) -> Result<StoreModuleInfo, String> {
    compile_store_module(src, "./mist-rt.js").map(|(_, info)| info)
}

pub fn config_literal_to_json(literal: &str) -> Result<String, String> {
    let allocator = Allocator::default();
    let src = format!("const __c = {};", literal);
    let ret = Parser::new(&allocator, &src, SourceType::default().with_typescript(true)).parse();
    if !ret.errors.is_empty() {
        return Err("config must be a static object literal".to_string());
    }
    for stmt in &ret.program.body {
        if let Statement::VariableDeclaration(var) = stmt {
            if let Some(init) = &var.declarations[0].init {
                return expr_to_json(unparenthesize(init), &src);
            }
        }
    }
    Err("config must be a static object literal".to_string())
}

fn expr_to_json(expr: &Expression, src: &str) -> Result<String, String> {
    match expr {
        Expression::ObjectExpression(obj) => {
            let mut parts = Vec::new();
            for prop in &obj.properties {
                let ObjectPropertyKind::ObjectProperty(p) = prop else {
                    return Err("config objects cannot use spreads".to_string());
                };
                let key = p.key.static_name().ok_or("config keys must be static")?;
                parts.push(format!(
                    "{}: {}",
                    json_string(&key),
                    expr_to_json(unparenthesize(&p.value), src)?
                ));
            }
            Ok(format!("{{ {} }}", parts.join(", ")))
        }
        Expression::ArrayExpression(arr) => {
            let mut parts = Vec::new();
            for el in &arr.elements {
                let e = el.as_expression().ok_or("config arrays must hold literals")?;
                parts.push(expr_to_json(unparenthesize(e), src)?);
            }
            Ok(format!("[{}]", parts.join(", ")))
        }
        Expression::StringLiteral(s) => Ok(json_string(&s.value)),
        Expression::NumericLiteral(n) => Ok(text(src, n.span).to_string()),
        Expression::BooleanLiteral(b) => Ok(b.value.to_string()),
        Expression::NullLiteral(_) => Ok("null".to_string()),
        Expression::UnaryExpression(u) if u.operator == UnaryOperator::UnaryNegation => {
            Ok(text(src, u.span).to_string())
        }
        _ => Err("config values must be literal strings/numbers/booleans/objects/arrays".to_string()),
    }
}

fn json_string(v: &str) -> String {
    let mut out = String::with_capacity(v.len() + 2);
    out.push('"');
    for c in v.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// (derived, wx:key) pairs to append to the analysis, with reads rewritten the
pub fn hoisted_deriveds(
    analysis: &Analysis,
    page_exprs: &[String],
    for_hoists: &[crate::wxml::ForHoist],
) -> Result<Vec<(DerivedDecl, Option<String>)>, String> {
    let mut reactive: Vec<String> =
        analysis.states.iter().filter(|s| s.bound).map(|s| s.name.clone()).collect();
    reactive.extend(analysis.deriveds.iter().map(|d| d.name.clone()));
    reactive.extend(analysis.store_imports.iter().flat_map(|si| si.stores.iter().cloned()));
    let unbound: Vec<String> =
        analysis.states.iter().filter(|s| !s.bound).map(|s| s.name.clone()).collect();
    let store_accessors: std::collections::BTreeMap<String, String> = analysis
        .store_imports
        .iter()
        .flat_map(|si| si.stores.iter().map(|st| (st.clone(), format!("{}.{}", si.alias, st))))
        .collect();
    let mut methods: Vec<String> = analysis.methods.iter().map(|m| m.name.clone()).collect();
    methods.extend(analysis.callback_props.iter().cloned());
    methods.extend(analysis.store_imports.iter().flat_map(|si| si.fns.iter().cloned()));
    let rw = Rewriter::new(&reactive, &methods, &store_accessors, &unbound)?;

    let mut out = Vec::new();
    for (i, e) in page_exprs.iter().enumerate() {
        out.push((
            DerivedDecl { name: format!("_h{}", i), arrow: format!("() => ({})", rw.rewrite_reads(e)) },
            None,
        ));
    }
    for fh in for_hoists {
        let fields: Vec<String> = fh
            .fields
            .iter()
            .enumerate()
            .map(|(i, f)| format!("_c{}: {}", i, rw.rewrite_reads(f)))
            .collect();
        let arrow = format!(
            "() => ({}).map({} => ({{ ...{}, {} }}))",
            rw.rewrite_reads(&fh.list),
            fh.param,
            fh.param,
            fields.join(", ")
        );
        out.push((DerivedDecl { name: fh.name.clone(), arrow }, fh.key.clone()));
    }
    Ok(out)
}

pub fn emit_app_js(analysis: &Analysis) -> String {
    let mut out = String::new();
    out.push_str("// generated by mistc — do not edit\n");
    out.push_str("const rt = require('./mist-rt.js');\n");
    out.push_str("rt.observePerf();\n");
    for stmt in &analysis.plain_stmts {
        out.push_str(stmt);
        out.push('\n');
    }
    out.push_str("App({\n");
    out.push_str("  __perf: rt.perfEntries,\n");
    for m in &analysis.methods {
        let prefix = if m.is_async { "async " } else { "" };
        out.push_str(&format!("  {}{}{} {},\n", prefix, m.name, m.params, m.body));
    }
    for l in &analysis.lifecycles {
        let prefix = if l.is_async { "async " } else { "" };
        out.push_str(&format!("  {}{}{} {},\n", prefix, l.hook, l.params, l.body));
    }
    out.push_str("});\n");
    out
}

fn emit_data(out: &mut String, analysis: &Analysis, pad: &str) {
    out.push_str(&format!("{}data: {{\n", pad));
    for s in analysis.states.iter().filter(|s| s.bound) {
        out.push_str(&format!("{}  {}: {},\n", pad, s.name, s.init));
    }
    for d in &analysis.deriveds {
        out.push_str(&format!("{}  {}: null,\n", pad, d.name));
    }
    for si in &analysis.store_imports {
        for s in &si.stores {
            out.push_str(&format!("{}  {}: null,\n", pad, s));
        }
    }
    out.push_str(&format!("{}}},\n", pad));
}

fn inner_of_block(body: &str) -> &str {
    let t = body.trim();
    match t.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        Some(inner) => inner.trim_matches('\n'),
        None => t,
    }
}

struct Rewriter {
    reads: Regex,
    calls: Option<Regex>,
    store_reads: Vec<(Regex, String)>,
    unbound_reads: Vec<(Regex, String)>,
}

impl Rewriter {
    fn new(
        reactive: &[String],
        methods: &[String],
        store_accessors: &std::collections::BTreeMap<String, String>,
        unbound: &[String],
    ) -> Result<Self, String> {
        let reads_pattern = if reactive.is_empty() {
            r"\b__none__\.value".to_string()
        } else {
            format!(r"\b({})\.value", reactive.join("|"))
        };
        let calls = if methods.is_empty() {
            None
        } else {
            Some(Regex::new(&format!(r"\b({})\s*\(", methods.join("|"))).map_err(|e| e.to_string())?)
        };
        // `(^|[^.\w])` guard: don't re-prefix reads already behind the module alias
        let store_reads = store_accessors
            .iter()
            .map(|(root, acc)| {
                Regex::new(&format!(r"(^|[^.\w]){}\.value", root))
                    .map(|re| (re, format!("${{1}}{}.value", acc)))
                    .map_err(|e| e.to_string())
            })
            .collect::<Result<Vec<_>, String>>()?;
        let unbound_reads = unbound
            .iter()
            .map(|root| {
                Regex::new(&format!(r"\b{}\.value", root))
                    .map(|re| (re, format!("this._{}", root)))
                    .map_err(|e| e.to_string())
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Rewriter {
            reads: Regex::new(&reads_pattern).map_err(|e| e.to_string())?,
            calls,
            store_reads,
            unbound_reads,
        })
    }

    fn rewrite_reads(&self, code: &str) -> String {
        let mut out = self.reads.replace_all(code, "this.data.$1").to_string();
        for (re, replacement) in &self.unbound_reads {
            out = re.replace_all(&out, replacement.as_str()).to_string();
        }
        for (re, replacement) in &self.store_reads {
            out = re.replace_all(&out, replacement.as_str()).to_string();
        }
        match &self.calls {
            Some(re) => re.replace_all(&out, "this.$1(").to_string(),
            None => out,
        }
    }

    fn transform(&self, src: &str, span: Span, edits: &[Edit]) -> String {
        let start = span.start;
        let end = span.end;
        let mut slice = text(src, span).to_string();
        let mut local: Vec<&Edit> = edits.iter().filter(|e| e.start >= start && e.end <= end).collect();
        local.sort_by(|a, b| b.start.cmp(&a.start));
        for e in local {
            let s = (e.start - start) as usize;
            let t = (e.end - start) as usize;
            slice.replace_range(s..t, &e.text);
        }
        self.rewrite_reads(&slice)
    }
}

struct MutationCollector<'s> {
    src: &'s str,
    states: Vec<String>,
    unbound: Vec<String>,
    stores: std::collections::BTreeMap<String, String>,
    edits: Vec<Edit>,
    errors: Vec<String>,
    line_offset: usize,
}

impl<'s> MutationCollector<'s> {
    fn state_path(&self, target_text: &str) -> Option<(String, String)> {
        let idx = target_text.find(".value")?;
        let root = &target_text[..idx];
        if !self.states.iter().any(|s| s == root) {
            return None;
        }
        let rest = &target_text[idx + 6..];
        Some((root.to_string(), rest.to_string()))
    }

    fn unbound_edit(&self, span: Span) -> Option<Edit> {
        let t = text(self.src, span);
        let idx = t.find(".value")?;
        let root = &t[..idx];
        let root_ident: String =
            root.chars().rev().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        let root_ident: String = root_ident.chars().rev().collect();
        if !self.unbound.contains(&root_ident) {
            return None;
        }
        let mut replaced = t.to_string();
        for u in &self.unbound {
            replaced = replaced.replace(&format!("{}.value", u), &format!("this._{}", u));
        }
        Some(Edit {
            start: span.start,
            end: span.end,
            text: format!(";({}, rt.touch(this))", replaced),
        })
    }

    fn store_path(&self, target_text: &str) -> Option<(String, String)> {
        let idx = target_text.find(".value")?;
        let root = &target_text[..idx];
        let acc = self.stores.get(root)?;
        let rest = &target_text[idx + 6..];
        Some((acc.clone(), rest.to_string()))
    }
}

fn store_path_expr(rest: &str) -> String {
    let rest = rest.strip_prefix('.').unwrap_or(rest);
    if rest.is_empty() {
        return "null".to_string();
    }
    let mut out = String::from("`");
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            let mut depth = 1;
            let mut inner = String::new();
            for c2 in chars.by_ref() {
                match c2 {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                if depth > 0 {
                    inner.push(c2);
                }
            }
            out.push_str(&format!("[${{{}}}]", inner));
        } else {
            out.push(c);
        }
    }
    out.push('`');
    out
}

fn path_expr(root: &str, rest: &str) -> String {
    if rest.is_empty() {
        return format!("'{}'", root);
    }
    let mut out = String::from("`");
    out.push_str(root);
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            let mut depth = 1;
            let mut inner = String::new();
            for c2 in chars.by_ref() {
                match c2 {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                if depth > 0 {
                    inner.push(c2);
                }
            }
            out.push_str(&format!("[${{{}}}]", inner));
        } else {
            out.push(c);
        }
    }
    out.push('`');
    out
}

fn read_expr(root: &str, rest: &str) -> String {
    format!("this.data.{}{}", root, rest)
}

impl<'a> Visit<'a> for MutationCollector<'_> {
    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        let left_span = it.left.span();
        let left_text = text(self.src, left_span);
        if let Some(edit) = self.unbound_edit(it.span) {
            self.edits.push(edit);
            walk::walk_assignment_expression(self, it);
            return;
        }
        if let Some((root, rest)) = self.state_path(left_text) {
            let path = path_expr(&root, &rest);
            let rhs = text(self.src, it.right.span());
            let replacement = match it.operator {
                AssignmentOperator::Assign => format!("this.__set({}, {})", path, rhs),
                op => {
                    let base = op.as_str().trim_end_matches('=');
                    format!("this.__set({}, {} {} ({}))", path, read_expr(&root, &rest), base, rhs)
                }
            };
            self.edits.push(Edit { start: it.span.start, end: it.span.end, text: replacement });
        } else if let Some((acc, rest)) = self.store_path(left_text) {
            let path = store_path_expr(&rest);
            let rhs = text(self.src, it.right.span());
            let replacement = match it.operator {
                AssignmentOperator::Assign => format!("{}.__set({}, {})", acc, path, rhs),
                op => {
                    let base = op.as_str().trim_end_matches('=');
                    format!("{}.__set({}, {}.value{} {} ({}))", acc, path, acc, rest, base, rhs)
                }
            };
            self.edits.push(Edit { start: it.span.start, end: it.span.end, text: replacement });
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_update_expression(&mut self, it: &UpdateExpression<'a>) {
        let target_text = text(self.src, it.argument.span());
        if self.unbound.iter().any(|u| target_text.starts_with(&format!("{}.value", u))) {
            if let Some(edit) = self.unbound_edit(it.span) {
                self.edits.push(edit);
                walk::walk_update_expression(self, it);
                return;
            }
        }
        if let Some((root, rest)) = self.state_path(target_text) {
            let path = path_expr(&root, &rest);
            let op = if it.operator == UpdateOperator::Increment { "+" } else { "-" };
            let replacement =
                format!("this.__set({}, {} {} 1)", path, read_expr(&root, &rest), op);
            self.edits.push(Edit { start: it.span.start, end: it.span.end, text: replacement });
        } else if let Some((acc, rest)) = self.store_path(target_text) {
            let path = store_path_expr(&rest);
            let op = if it.operator == UpdateOperator::Increment { "+" } else { "-" };
            let replacement = format!("{}.__set({}, {}.value{} {} 1)", acc, path, acc, rest, op);
            self.edits.push(Edit { start: it.span.start, end: it.span.end, text: replacement });
        }
        walk::walk_update_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Some(member) = it.callee.as_member_expression() {
            let obj_text = text(self.src, member.object().span());
            let unbound_target = self.unbound.iter().any(|u| obj_text.starts_with(&format!("{}.value", u)));
            if unbound_target {
                if let Some(m) = member.static_property_name() {
                    if ["push", "pop", "splice", "shift", "unshift", "sort", "reverse"].contains(&m) {
                        if let Some(edit) = self.unbound_edit(it.span) {
                            self.edits.push(edit);
                        }
                    }
                }
                walk::walk_call_expression(self, it);
                return;
            }
            if let Some((root, rest)) = self.state_path(obj_text) {
                if rest.contains('(') {
                    walk::walk_call_expression(self, it);
                    return;
                }
                if let Some(method) = member.static_property_name() {
                    match method {
                        "push" => {
                            let arg = it
                                .arguments
                                .first()
                                .map(|a| text(self.src, a.span()).to_string())
                                .unwrap_or_default();
                            let read = read_expr(&root, &rest);
                            let path = path_expr(&root, &format!("{}[__L__]", rest));
                            let path = path.replace("[${__L__}]", &format!("[${{{}.length}}]", read));
                            let replacement = format!("this.__set({}, {})", path, arg);
                            self.edits.push(Edit { start: it.span.start, end: it.span.end, text: replacement });
                        }
                        "pop" | "splice" | "shift" | "unshift" | "sort" | "reverse" => {
                            let (line, col) = line_col(self.src, it.span.start as usize, self.line_offset);
                            self.errors.push(format!(
                                "M1004 at line {}:{}: `{}.value{}.{}()` — only push/index assignment compile to precise writes\n  help: reassign `{}.value = ...` instead",
                                line, col, root, rest, method, root
                            ));
                        }
                        _ => {}
                    }
                }
            } else if let Some((acc, rest)) = self.store_path(obj_text) {
                if rest.contains('(') {
                    walk::walk_call_expression(self, it);
                    return;
                }
                if let Some(method) = member.static_property_name() {
                    match method {
                        "push" => {
                            let arg = it
                                .arguments
                                .first()
                                .map(|a| text(self.src, a.span()).to_string())
                                .unwrap_or_default();
                            let read = format!("{}.value{}", acc, rest);
                            let path = store_path_expr(&format!("{}[__L__]", rest));
                            let path = path.replace("[${__L__}]", &format!("[${{{}.length}}]", read));
                            let replacement = format!("{}.__set({}, {})", acc, path, arg);
                            self.edits.push(Edit { start: it.span.start, end: it.span.end, text: replacement });
                        }
                        "pop" | "splice" | "shift" | "unshift" | "sort" | "reverse" => {
                            let (line, col) = line_col(self.src, it.span.start as usize, self.line_offset);
                            self.errors.push(format!(
                                "M1004 at line {}:{}: store `.value{}.{}()` — only push/index assignment compile to precise writes\n  help: reassign `.value = ...` instead",
                                line, col, rest, method
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
        walk::walk_call_expression(self, it);
    }
}

fn binding_name(pat: &BindingPattern) -> Option<String> {
    match &pat.kind {
        BindingPatternKind::BindingIdentifier(id) => Some(id.name.to_string()),
        _ => None,
    }
}

fn callee_name<'a>(call: &'a CallExpression) -> Option<&'a str> {
    match &call.callee {
        Expression::Identifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

fn unparenthesize<'a, 'b>(expr: &'b Expression<'a>) -> &'b Expression<'a> {
    match expr {
        Expression::ParenthesizedExpression(p) => unparenthesize(&p.expression),
        other => other,
    }
}

fn text(src: &str, span: Span) -> &str {
    &src[span.start as usize..span.end as usize]
}
