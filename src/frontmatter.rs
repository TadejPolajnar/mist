use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_ast::visit::walk;
use oxc_ast::Visit;
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType, Span};
use oxc_syntax::scope::ScopeFlags;
use regex::Regex;

use crate::template;
use crate::wxml::Handler;

pub struct Analysis {
    pub states: Vec<StateDecl>,
    pub deriveds: Vec<DerivedDecl>,
    pub methods: Vec<Method>,
    pub lifecycles: Vec<Lifecycle>,
    pub config: Option<String>,
    pub inline: Option<bool>,
    pub plain_stmts: Vec<String>,
    pub plain_consts: Vec<String>,
    pub imports: Vec<MistImport>,
    pub store_imports: Vec<StoreImport>,
    pub data_props: Vec<PropDecl>,
    pub callback_props: Vec<String>,
    pub event_options: Vec<(String, String)>,
    pub plugin_imports: Vec<PluginImport>,
    pub plugin_components: Vec<(String, String)>,
    /// `config.customTags` — tag names that suppress the M1019 unknown-tag warning
    pub custom_tags: Vec<String>,
    /// `config.virtualHost` — component-only; removes the wrapper node
    pub virtual_host: Option<bool>,
    /// `config.pureDataPattern` — component-only; regex source (validated, no `/`)
    pub pure_data_pattern: Option<String>,
    /// `config.externalClasses` — component-only; parent-styleable class names
    pub external_classes: Vec<String>,
    /// `navigate(...)`/`navigate.replace/switchTab(...)` call sites with literal
    /// routes — validated against the project's route set once it exists (M1021)
    pub route_refs: Vec<RouteRef>,
}

/// A default import of a WeChat plugin, e.g. `import cal from 'plugin://calendar'`.
pub struct PluginImport {
    pub local: String,
    pub plugin: String,
}

pub struct MistImport {
    pub local: String,
    pub path: String,
}

/// Exports of a compiled `stores/*.ts` module.
#[derive(Clone, Debug, Default)]
pub struct StoreModuleInfo {
    pub stores: Vec<String>,
    pub fns: Vec<String>,
}

/// A page/component's import of a store module.
pub struct StoreImport {
    pub path: String,
    /// generated local binding for the require, e.g. `__S0`
    pub alias: String,
    /// store boxes imported from the module
    pub stores: Vec<String>,
    /// functions imported from the module
    pub fns: Vec<String>,
    /// filled by the caller once the output layout is known
    pub require_path: String,
}

pub struct PropDecl {
    pub name: String,
    pub default: Option<String>,
    pub prop_type: PropType,
}

/// WeChat property `type:` value inferred from a `props<{...}>()` type argument.
/// `Unknown` emits `type: null` — today's behavior, and the fallback for any
/// type shape the mapper can't confidently classify.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PropType {
    String,
    Number,
    Boolean,
    Array,
    Object,
    Unknown,
}

impl PropType {
    fn as_wx(self) -> &'static str {
        match self {
            PropType::String => "String",
            PropType::Number => "Number",
            PropType::Boolean => "Boolean",
            PropType::Array => "Array",
            PropType::Object => "Object",
            PropType::Unknown => "null",
        }
    }
}

pub struct StateDecl {
    pub name: String,
    pub init: String,
    /// referenced by the template? unbound states become instance fields (`this._x`)
    /// and never enter `data` — zero bridge cost (SPEC §8.5 dead-data elimination)
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
    pub line: usize,
    pub col: usize,
}

const LIFECYCLE_HOOKS: &[&str] = &[
    "onLoad", "onShow", "onReady", "onHide", "onUnload", "onAttach", "onDetach", "onLaunch",
    "onPullDownRefresh", "onReachBottom", "onPageScroll", "onTabItemTap", "onResize",
    "onPageShow", "onPageHide", "onShareAppMessage", "onShareTimeline", "onAddToFavorites",
    "onError", "onPageNotFound", "onUnhandledRejection", "onThemeChange", "onCreate", "onMove",
    "onRouteDone", "onSaveExitState",
];

/// Hooks whose callback's value is returned to WeChat (share/favorite configs).
const RETURNING_HOOKS: &[&str] =
    &["onShareAppMessage", "onShareTimeline", "onAddToFavorites", "onSaveExitState"];

/// Component-only hooks that map into WeChat's `pageLifetimes` block.
const PAGE_LIFETIME_HOOKS: &[(&str, &str)] =
    &[("onPageShow", "show"), ("onPageHide", "hide"), ("onResize", "resize")];

const MIST_VALUE_EXPORTS: &[&str] = &["state", "derived", "store", "props", "navigate"];

/// A `navigate(...)`-family call site collected during analysis for late
/// (project-level) route validation — the route set only exists once every
/// page/subpackage page has been discovered (`compile_project_dir`).
#[derive(Clone, Debug)]
pub struct RouteRef {
    pub route: String,
    pub kind: RouteRefKind,
    pub line: usize,
    pub col: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteRefKind {
    /// `navigate(route, params?)` → `wx.navigateTo`
    Push,
    /// `navigate.replace(route, params?)` → `wx.redirectTo`
    Replace,
    /// `navigate.switchTab(route)` → `wx.switchTab` — must be a tab-bar page when known
    SwitchTab,
}

fn check_mist_specifiers(
    src: &str,
    specifiers: &[ImportDeclarationSpecifier],
    line_offset: usize,
) -> Result<(), String> {
    for s in specifiers {
        let ImportDeclarationSpecifier::ImportSpecifier(named) = s else {
            let (line, col) = line_col(src, s.span().start as usize, line_offset);
            return Err(format!(
                "M1011 at line {}:{}: 'mist' has no default or namespace export — use named imports like `import {{ state }} from 'mist'`",
                line, col
            ));
        };
        if named.import_kind.is_type() {
            continue;
        }
        let imported = named.imported.name().to_string();
        let local = named.local.name.to_string();
        if imported != local {
            let (line, col) = line_col(src, named.span.start as usize, line_offset);
            return Err(format!(
                "M1011 at line {}:{}: mist imports cannot be aliased (`{} as {}`) — the compiler matches them by name",
                line, col, imported, local
            ));
        }
        let known = MIST_VALUE_EXPORTS.iter().any(|k| *k == imported)
            || LIFECYCLE_HOOKS.iter().any(|k| *k == imported);
        if !known {
            let (line, col) = line_col(src, named.span.start as usize, line_offset);
            return Err(format!(
                "M1011 at line {}:{}: 'mist' has no export `{}`\n  help: available: {}, and lifecycle hooks: {}. If `{}` is a WeChat hook mist doesn't support yet, it needs compiler support — using it would silently break at runtime",
                line,
                col,
                imported,
                MIST_VALUE_EXPORTS.join(", "),
                LIFECYCLE_HOOKS.join(", "),
                imported
            ));
        }
    }
    Ok(())
}

fn is_valid_plugin_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[derive(Clone)]
struct Edit {
    start: u32,
    end: u32,
    text: String,
}

pub fn analyze(src: &str) -> Result<Analysis, String> {
    analyze_with_stores_bound(src, &|_| None, 1, None)
}

/// (line, col), both 1-based, of a byte offset within `src`, shifted so that
/// `src`'s first line is `line_offset` in the enclosing file.
pub fn line_col(src: &str, byte: usize, line_offset: usize) -> (usize, usize) {
    let byte = byte.min(src.len());
    let line = src[..byte].matches('\n').count() + line_offset;
    let col = byte - src[..byte].rfind('\n').map(|i| i + 1).unwrap_or(0) + 1;
    (line, col)
}

/// `resolve` maps a relative non-`.mist` import path to the store module's exports
/// (project builds read and analyze the file; standalone unit builds resolve nothing).
/// `line_offset`: 1-based line of `src`'s first line within the `.mist` file.
pub fn analyze_with_stores(
    src: &str,
    resolve: &dyn Fn(&str) -> Option<StoreModuleInfo>,
    line_offset: usize,
) -> Result<Analysis, String> {
    analyze_with_stores_bound(src, resolve, line_offset, None)
}

/// `template`: when given, states not referenced in it are compiled as instance
/// fields instead of `data` keys (dead-data elimination).
struct TypeScan<'s> {
    src: &'s str,
    spans: Vec<(u32, u32)>,
    enum_span: Option<u32>,
}

impl<'a> Visit<'a> for TypeScan<'_> {
    fn visit_ts_type_annotation(&mut self, it: &TSTypeAnnotation<'a>) {
        self.spans.push((it.span.start, it.span.end));
    }

    fn visit_ts_interface_declaration(&mut self, it: &TSInterfaceDeclaration<'a>) {
        self.spans.push((it.span.start, it.span.end));
    }

    fn visit_ts_type_alias_declaration(&mut self, it: &TSTypeAliasDeclaration<'a>) {
        self.spans.push((it.span.start, it.span.end));
    }

    fn visit_ts_enum_declaration(&mut self, it: &TSEnumDeclaration<'a>) {
        if self.enum_span.is_none() {
            self.enum_span = Some(it.span.start);
        }
    }

    fn visit_ts_as_expression(&mut self, it: &TSAsExpression<'a>) {
        self.spans.push((it.expression.span().end, it.span.end));
        walk::walk_ts_as_expression(self, it);
    }

    fn visit_ts_satisfies_expression(&mut self, it: &TSSatisfiesExpression<'a>) {
        self.spans.push((it.expression.span().end, it.span.end));
        walk::walk_ts_satisfies_expression(self, it);
    }

    fn visit_ts_non_null_expression(&mut self, it: &TSNonNullExpression<'a>) {
        self.spans.push((it.span.end - 1, it.span.end));
        walk::walk_ts_non_null_expression(self, it);
    }

    fn visit_ts_type_parameter_instantiation(&mut self, it: &TSTypeParameterInstantiation<'a>) {
        self.spans.push((it.span.start, it.span.end));
    }

    fn visit_ts_type_parameter_declaration(&mut self, it: &TSTypeParameterDeclaration<'a>) {
        self.spans.push((it.span.start, it.span.end));
    }

    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        if it.import_kind.is_type() {
            self.spans.push((it.span.start, it.span.end));
            return;
        }
        walk::walk_import_declaration(self, it);
    }

    fn visit_formal_parameter(&mut self, it: &FormalParameter<'a>) {
        if it.pattern.optional {
            let start = it.span.start as usize;
            let end = it
                .pattern
                .type_annotation
                .as_ref()
                .map(|a| a.span.start as usize)
                .unwrap_or(it.span.end as usize)
                .min(self.src.len());
            if let Some(pos) = self.src.get(start..end).and_then(|s| s.rfind('?')) {
                let p = (start + pos) as u32;
                self.spans.push((p, p + 1));
            }
        }
        walk::walk_formal_parameter(self, it);
    }
}

/// Finds the first `props<...>()` call and records its type-argument source text,
/// captured before `strip_types` blanks it out.
struct PropsTypeArgScan<'s> {
    src: &'s str,
    type_arg_text: Option<String>,
}

impl<'a> Visit<'a> for PropsTypeArgScan<'_> {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if self.type_arg_text.is_none() {
            if callee_name(it) == Some("props") {
                if let Some(type_params) = &it.type_parameters {
                    self.type_arg_text = Some(text(self.src, type_params.span).to_string());
                }
            }
        }
        walk::walk_call_expression(self, it);
    }
}

/// Maps each member of a `props<{...}>()` type argument to a WeChat property type.
/// Returns `None` if the captured text isn't a single object type literal (or
/// fails to parse) — callers then fall back to `type: null` for every prop.
fn map_props_type_arg(type_arg_text: &str) -> Option<std::collections::HashMap<String, PropType>> {
    let inner = type_arg_text.strip_prefix('<')?.strip_suffix('>')?;
    let wrapped = format!("type __T = {};", inner);
    let allocator = Allocator::default();
    let source_type = SourceType::default().with_typescript(true);
    let ret = Parser::new(&allocator, &wrapped, source_type).parse();
    if !ret.errors.is_empty() {
        return None;
    }
    for stmt in &ret.program.body {
        let Statement::TSTypeAliasDeclaration(alias) = stmt else { continue };
        if alias.id.name != "__T" {
            continue;
        }
        let TSType::TSTypeLiteral(lit) = &alias.type_annotation else {
            return None;
        };
        let mut out = std::collections::HashMap::new();
        for member in &lit.members {
            let TSSignature::TSPropertySignature(sig) = member else { continue };
            let Some(name) = sig.key.static_name() else { continue };
            let ty = sig
                .type_annotation
                .as_ref()
                .map(|ann| classify_ts_type(&ann.type_annotation))
                .unwrap_or(PropType::Unknown);
            out.insert(name.to_string(), ty);
        }
        return Some(out);
    }
    None
}

/// Classifies a single `TSType` into a WeChat property type per Plan 019's table.
/// Mixed unions, generics, `any`/`unknown`, and unresolvable references fall back
/// to `Unknown` (`type: null`) rather than ever erroring.
fn classify_ts_type(ty: &TSType) -> PropType {
    match ty {
        TSType::TSStringKeyword(_) => PropType::String,
        TSType::TSNumberKeyword(_) => PropType::Number,
        TSType::TSBooleanKeyword(_) => PropType::Boolean,
        TSType::TSLiteralType(lit) => match &lit.literal {
            TSLiteral::StringLiteral(_) | TSLiteral::TemplateLiteral(_) => PropType::String,
            TSLiteral::NumericLiteral(_) | TSLiteral::BigIntLiteral(_) => PropType::Number,
            TSLiteral::BooleanLiteral(_) => PropType::Boolean,
            _ => PropType::Unknown,
        },
        TSType::TSArrayType(_) | TSType::TSTupleType(_) => PropType::Array,
        TSType::TSTypeLiteral(_) => PropType::Object,
        TSType::TSParenthesizedType(p) => classify_ts_type(&p.type_annotation),
        TSType::TSUnionType(u) => {
            let mut kind: Option<PropType> = None;
            for member in &u.types {
                let m = classify_ts_type(member);
                if m == PropType::Unknown {
                    return PropType::Unknown;
                }
                match kind {
                    None => kind = Some(m),
                    Some(k) if k == m => {}
                    Some(_) => return PropType::Unknown,
                }
            }
            kind.unwrap_or(PropType::Unknown)
        }
        TSType::TSTypeReference(r) => {
            let TSTypeName::IdentifierReference(id) = &r.type_name else {
                return PropType::Object;
            };
            match id.name.as_str() {
                "Array" => PropType::Array,
                "Record" => PropType::Object,
                "string" | "number" | "boolean" | "any" | "unknown" | "never" | "object" => {
                    PropType::Unknown
                }
                _ => PropType::Object,
            }
        }
        _ => PropType::Unknown,
    }
}

/// `strip_types` result: the whitespace-blanked source, plus any `props<{...}>()`
/// member→type map captured from the original (pre-blanking) AST.
struct StrippedSrc {
    src: String,
    prop_types: Option<std::collections::HashMap<String, PropType>>,
}

fn strip_types(src: &str, line_offset: usize) -> Result<StrippedSrc, String> {
    let allocator = Allocator::default();
    let source_type = SourceType::default().with_typescript(true);
    let ret = Parser::new(&allocator, src, source_type).parse();
    if !ret.errors.is_empty() {
        return Ok(StrippedSrc { src: src.to_string(), prop_types: None });
    }
    let mut props_scan = PropsTypeArgScan { src, type_arg_text: None };
    props_scan.visit_program(&ret.program);
    let prop_types = props_scan.type_arg_text.and_then(|t| map_props_type_arg(&t));

    let mut scan = TypeScan { src, spans: Vec::new(), enum_span: None };
    scan.visit_program(&ret.program);
    if let Some(start) = scan.enum_span {
        let (line, col) = line_col(src, start as usize, line_offset);
        return Err(format!(
            "TS enum at line {}:{} is not supported — enums are runtime constructs the compiler does not emit\n  help: use a const object (`const Status = {{ Open: 0, Done: 1 }}`) or a string-literal union type",
            line, col
        ));
    }
    if scan.spans.is_empty() {
        return Ok(StrippedSrc { src: src.to_string(), prop_types });
    }
    let mut out: Vec<u8> = src.as_bytes().to_vec();
    for (start, end) in &scan.spans {
        for i in *start as usize..(*end as usize).min(out.len()) {
            if out[i] != b'\n' {
                out[i] = b' ';
            }
        }
    }
    let src = String::from_utf8(out).unwrap_or_else(|_| src.to_string());
    Ok(StrippedSrc { src, prop_types })
}

pub fn analyze_with_stores_bound(
    src: &str,
    resolve: &dyn Fn(&str) -> Option<StoreModuleInfo>,
    line_offset: usize,
    template: Option<&str>,
) -> Result<Analysis, String> {
    let stripped = strip_types(src, line_offset)?;
    let src = stripped.src.as_str();
    let prop_types = stripped.prop_types;
    let allocator = Allocator::default();
    let source_type = SourceType::default().with_typescript(true);
    let ret = Parser::new(&allocator, src, source_type).parse();
    if !ret.errors.is_empty() {
        let msgs: Vec<String> = ret.errors.iter().map(|e| e.to_string()).collect();
        return Err(format!("frontmatter parse errors: {}", msgs.join("; ")));
    }
    let program = ret.program;

    // pass 1: reactive names
    let mut states: Vec<StateDecl> = Vec::new();
    let mut deriveds: Vec<(String, Span)> = Vec::new();
    let mut raw_methods: Vec<(String, Span, Span, bool)> = Vec::new(); // name, params-ish span gap, body span
    let mut raw_lifecycles: Vec<(String, Span, Span, bool, bool, u32)> = Vec::new();
    let mut config: Option<String> = None;
    let mut inline_flag: Option<bool> = None;
    let mut events_raw: Option<String> = None;
    let mut plugin_components_raw: Option<String> = None;
    let mut custom_tags_raw: Option<String> = None;
    let mut component_options_raw: Option<String> = None;
    let mut plain_stmts: Vec<Span> = Vec::new();
    let mut plain_consts: Vec<String> = Vec::new();
    let mut imports: Vec<MistImport> = Vec::new();
    let mut data_props: Vec<PropDecl> = Vec::new();
    let mut callback_props: Vec<String> = Vec::new();
    let mut store_imports: Vec<StoreImport> = Vec::new();
    let mut plugin_imports: Vec<PluginImport> = Vec::new();

    for stmt in &program.body {
        match stmt {
            Statement::ImportDeclaration(import) => {
                let path = import.source.value.to_string();
                if let Some(plugin) = path.strip_prefix("plugin://") {
                    let (line, col) = line_col(src, import.span.start as usize, line_offset);
                    if !is_valid_plugin_name(plugin) {
                        return Err(format!(
                            "M1015 at line {}:{}: invalid plugin specifier '{}' — expected 'plugin://<name>' with an alphanumeric/-/_ name",
                            line, col, path
                        ));
                    }
                    let Some(specs) = import.specifiers.as_ref() else {
                        return Err(format!(
                            "M1015 at line {}:{}: plugin import '{}' needs a default import — `import name from '{}'`",
                            line, col, path, path
                        ));
                    };
                    let local = specs.iter().find_map(|s| match s {
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(d) => {
                            Some(d.local.name.to_string())
                        }
                        _ => None,
                    });
                    let Some(local) = local else {
                        return Err(format!(
                            "M1015 at line {}:{}: plugin import '{}' supports only a default import of the whole plugin",
                            line, col, path
                        ));
                    };
                    if specs.len() != 1 {
                        return Err(format!(
                            "M1015 at line {}:{}: plugin import '{}' supports only a default import of the whole plugin",
                            line, col, path
                        ));
                    }
                    plugin_imports.push(PluginImport { local, plugin: plugin.to_string() });
                } else if path.ends_with(".mist") {
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
                } else if path != "mist" {
                    return Err(format!(
                        "cannot import '{}' — npm packages are not supported; only 'mist', relative store modules and .mist components can be imported",
                        path
                    ));
                } else if let Some(specs) = import.specifiers.as_ref() {
                    check_mist_specifiers(src, specs, line_offset)?;
                }
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(Declaration::VariableDeclaration(var)) = &export.declaration {
                    for decl in &var.declarations {
                        if binding_name(&decl.id).as_deref() == Some("config") {
                            if let Some(init) = &decl.init {
                                let (flag, remaining) =
                                    split_inline_config(text(src, init.span()))?;
                                inline_flag = flag;
                                let (events, remaining) = match remaining {
                                    Some(raw) => split_events_config(&raw)?,
                                    None => (None, None),
                                };
                                events_raw = events;
                                let (plugin_comps, remaining) = match remaining {
                                    Some(raw) => split_plugin_components_config(&raw)?,
                                    None => (None, None),
                                };
                                plugin_components_raw = plugin_comps;
                                let (custom_tags, remaining) = match remaining {
                                    Some(raw) => split_custom_tags_config(&raw)?,
                                    None => (None, None),
                                };
                                custom_tags_raw = custom_tags;
                                let (component_options, remaining) = match remaining {
                                    Some(raw) => split_component_options_config(&raw)?,
                                    None => (None, None),
                                };
                                component_options_raw = component_options;
                                config = remaining;
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
                                    let prop_type = prop_types
                                        .as_ref()
                                        .and_then(|m| m.get(&key))
                                        .copied()
                                        .unwrap_or(PropType::Unknown);
                                    data_props.push(PropDecl { name: key, default, prop_type });
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
                                    states.push(StateDecl { name, init, bound: false });
                                    handled = true;
                                    continue;
                                }
                                "derived" => {
                                    let span = call
                                        .arguments
                                        .first()
                                        .map(|a| a.span())
                                        .ok_or("derived() requires a function argument")?;
                                    deriveds.push((name, span));
                                    handled = true;
                                    continue;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                if !handled {
                    for decl in &var.declarations {
                        if let BindingPatternKind::BindingIdentifier(id) = &decl.id.kind {
                            plain_consts.push(id.name.to_string());
                        }
                    }
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
                                call.span.start,
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

    // real `.value` reads only count in expression position (Expr text, Expr attrs,
    // For.list, If.cond) — a static attr string that merely contains "name.value"
    // (e.g. data-x="foo.value") is not a read. Line numbers don't matter here since
    // this only computes `bound`; the real template parse (with correct diagnostics)
    // happens later in the pipeline.
    if let Some(blob) = template {
        if let Ok(nodes) = template::parse_at(blob, 1) {
            let names: Vec<String> = states.iter().map(|s| s.name.clone()).collect();
            let reads = template::collect_expr_reads(&nodes, &names);
            let bind_targets = template::collect_bind_targets(&nodes);
            for s in states.iter_mut() {
                if reads.contains(&s.name) || bind_targets.contains(&s.name) {
                    s.bound = true;
                }
            }
        }
    }
    let state_names: Vec<String> = states.iter().map(|s| s.name.clone()).collect();
    let bound_names: Vec<String> =
        states.iter().filter(|s| s.bound).map(|s| s.name.clone()).collect();
    let unbound_names: Vec<String> =
        states.iter().filter(|s| !s.bound).map(|s| s.name.clone()).collect();
    let derived_names: Vec<String> = deriveds.iter().map(|(n, _)| n.clone()).collect();
    let mut reactive: Vec<String> = bound_names.clone();
    reactive.extend(derived_names.clone());
    let _ = &state_names;

    // one namespace: state, derived, props, methods, store boxes and store fns —
    // a duplicate silently loses one meaning (last object key wins), so reject it
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
            check(&d.0, "derived")?;
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
        for pi in &plugin_imports {
            check(&pi.local, "plugin import")?;
        }
    }

    // store root → accessor through the module require alias, e.g. `__S0.user`
    let store_accessors: std::collections::BTreeMap<String, String> = store_imports
        .iter()
        .flat_map(|si| si.stores.iter().map(|s| (s.clone(), format!("{}.{}", si.alias, s))))
        .collect();

    let mut alias_roots: Vec<String> = bound_names.clone();
    alias_roots.extend(unbound_names.clone());
    alias_roots.extend(store_accessors.keys().cloned());
    let mut value_roots = alias_roots.clone();
    value_roots.extend(derived_names.clone());
    let prop_roots: Vec<String> = data_props.iter().map(|p| p.name.clone()).collect();
    let mut alias_scan = AliasScan::new(src, alias_roots, value_roots, prop_roots);
    alias_scan.visit_program(&program);
    let value_errors = alias_scan.value_errors(line_offset);
    if !value_errors.is_empty() {
        return Err(value_errors.join("; "));
    }
    let prop_edits = alias_scan.prop_edits();
    let aliases = alias_scan.finish();

    // pass 2: collect mutation edits across the whole program
    let mut collector = MutationCollector {
        src,
        states: bound_names.clone(),
        unbound: unbound_names.clone(),
        stores: store_accessors.clone(),
        aliases,
        prop_rewrites: prop_edits.clone(),
        edits: Vec::new(),
        errors: Vec::new(),
        line_offset,
        route_refs: Vec::new(),
    };
    collector.visit_program(&program);
    if !collector.errors.is_empty() {
        return Err(collector.errors.join("; "));
    }

    // M1017: `created` runs before `properties`/`data` exist on the instance —
    // any state write in onCreate's callback would target an object that isn't there yet.
    for (hook, _, body_span, ..) in &raw_lifecycles {
        if hook != "onCreate" {
            continue;
        }
        if let Some(mutation) =
            collector.edits.iter().find(|e| e.start >= body_span.start && e.end <= body_span.end)
        {
            let (line, col) = line_col(src, mutation.start as usize, line_offset);
            return Err(format!(
                "M1017 at line {}:{}: state write inside onCreate — `created` runs before properties/data exist on the instance\n  help: move the write to onAttach, or use onCreate only to seed non-reactive instance fields",
                line, col
            ));
        }
    }

    let route_refs = collector.route_refs;
    let mut edits = collector.edits;
    let taken: Vec<(u32, u32)> = edits.iter().map(|e| (e.start, e.end)).collect();
    edits.extend(
        prop_edits
            .into_iter()
            .filter(|p| !taken.iter().any(|(s, e)| p.start < *e && *s < p.end)),
    );

    let mut method_names: Vec<String> = raw_methods.iter().map(|(n, ..)| n.clone()).collect();
    method_names.extend(callback_props.iter().cloned());
    // imported store fns get wrapper methods, so this.<fn>() works everywhere
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
        .map(|(hook, params_span, body_span, is_async, expr_body, start)| {
            let (line, col) = line_col(src, start as usize, line_offset);
            let params = text(src, params_span).trim().to_string();
            let params = if params.starts_with('(') { params } else { format!("({})", params) };
            let body = rewriter.transform(src, body_span, &edits);
            // expression-body arrows need braces to be valid as a method body;
            // share-type hooks must hand their value back to WeChat
            let body = if expr_body {
                if RETURNING_HOOKS.contains(&hook.as_str()) {
                    format!("{{ return {}; }}", body)
                } else {
                    format!("{{ {} }}", body)
                }
            } else {
                body
            };
            Lifecycle { hook, params, body, is_async, line, col }
        })
        .collect();

    let deriveds = deriveds
        .into_iter()
        .map(|(name, span)| DerivedDecl { arrow: rewriter.transform(src, span, &edits), name })
        .collect();

    let plain_stmts = plain_stmts
        .into_iter()
        .map(|span| text(src, span).to_string())
        .collect();

    let event_options = match events_raw {
        Some(raw) => {
            let entries = events_config_entries(&raw)?;
            for (name, _) in &entries {
                if !callback_props.contains(name) {
                    return Err(format!(
                        "config.events references '{}', which is not a declared callback prop — available: {}",
                        name,
                        callback_props.join(", ")
                    ));
                }
            }
            entries
        }
        None => Vec::new(),
    };

    let plugin_components = match plugin_components_raw {
        Some(raw) => plugin_components_config_entries(&raw)?,
        None => Vec::new(),
    };

    let custom_tags = match custom_tags_raw {
        Some(raw) => custom_tags_config_entries(&raw)?,
        None => Vec::new(),
    };

    let component_options = match component_options_raw {
        Some(raw) => component_options_config_entries(&raw)?,
        None => ComponentOptions::default(),
    };

    Ok(Analysis {
        states,
        deriveds,
        methods,
        lifecycles,
        config,
        inline: inline_flag,
        plain_stmts,
        plain_consts,
        imports,
        store_imports,
        data_props,
        callback_props,
        event_options,
        plugin_imports,
        plugin_components,
        custom_tags,
        virtual_host: component_options.virtual_host,
        pure_data_pattern: component_options.pure_data_pattern,
        external_classes: component_options.external_classes,
        route_refs,
    })
}

fn split_inline_config(raw: &str) -> Result<(Option<bool>, Option<String>), String> {
    let wrapped = format!("({})", raw);
    let allocator = Allocator::default();
    let source_type = SourceType::default().with_typescript(true);
    let ret = Parser::new(&allocator, &wrapped, source_type).parse();
    if !ret.errors.is_empty() {
        return Ok((None, Some(raw.to_string())));
    }
    let Some(Statement::ExpressionStatement(es)) = ret.program.body.first() else {
        return Ok((None, Some(raw.to_string())));
    };
    let Expression::ObjectExpression(obj) = unparenthesize(&es.expression) else {
        return Ok((None, Some(raw.to_string())));
    };
    let mut flag = None;
    let mut kept: Vec<String> = Vec::new();
    for p in &obj.properties {
        let mut is_inline = false;
        if let ObjectPropertyKind::ObjectProperty(op) = p {
            if op.key.static_name().as_deref() == Some("inline") {
                is_inline = true;
                let Expression::BooleanLiteral(b) = unparenthesize(&op.value) else {
                    return Err(
                        "config.inline must be the literal true or false".to_string()
                    );
                };
                flag = Some(b.value);
            }
        }
        if !is_inline {
            let span = p.span();
            kept.push(wrapped[span.start as usize..span.end as usize].to_string());
        }
    }
    if flag.is_none() {
        return Ok((None, Some(raw.to_string())));
    }
    if kept.is_empty() {
        return Ok((flag, None));
    }
    Ok((flag, Some(format!("{{ {} }}", kept.join(", ")))))
}

fn split_events_config(raw: &str) -> Result<(Option<String>, Option<String>), String> {
    let wrapped = format!("({})", raw);
    let allocator = Allocator::default();
    let source_type = SourceType::default().with_typescript(true);
    let ret = Parser::new(&allocator, &wrapped, source_type).parse();
    if !ret.errors.is_empty() {
        return Ok((None, Some(raw.to_string())));
    }
    let Some(Statement::ExpressionStatement(es)) = ret.program.body.first() else {
        return Ok((None, Some(raw.to_string())));
    };
    let Expression::ObjectExpression(obj) = unparenthesize(&es.expression) else {
        return Ok((None, Some(raw.to_string())));
    };
    let mut events = None;
    let mut kept: Vec<String> = Vec::new();
    for p in &obj.properties {
        let mut is_events = false;
        if let ObjectPropertyKind::ObjectProperty(op) = p {
            if op.key.static_name().as_deref() == Some("events") {
                is_events = true;
                let Expression::ObjectExpression(_) = unparenthesize(&op.value) else {
                    return Err("config.events must be an object literal".to_string());
                };
                events = Some(text(&wrapped, op.value.span()).to_string());
            }
        }
        if !is_events {
            let span = p.span();
            kept.push(wrapped[span.start as usize..span.end as usize].to_string());
        }
    }
    if events.is_none() {
        return Ok((None, Some(raw.to_string())));
    }
    if kept.is_empty() {
        return Ok((events, None));
    }
    Ok((events, Some(format!("{{ {} }}", kept.join(", ")))))
}

fn split_plugin_components_config(raw: &str) -> Result<(Option<String>, Option<String>), String> {
    let wrapped = format!("({})", raw);
    let allocator = Allocator::default();
    let source_type = SourceType::default().with_typescript(true);
    let ret = Parser::new(&allocator, &wrapped, source_type).parse();
    if !ret.errors.is_empty() {
        return Ok((None, Some(raw.to_string())));
    }
    let Some(Statement::ExpressionStatement(es)) = ret.program.body.first() else {
        return Ok((None, Some(raw.to_string())));
    };
    let Expression::ObjectExpression(obj) = unparenthesize(&es.expression) else {
        return Ok((None, Some(raw.to_string())));
    };
    let mut plugin_components = None;
    let mut kept: Vec<String> = Vec::new();
    for p in &obj.properties {
        let mut is_plugin_components = false;
        if let ObjectPropertyKind::ObjectProperty(op) = p {
            if op.key.static_name().as_deref() == Some("pluginComponents") {
                is_plugin_components = true;
                let Expression::ObjectExpression(_) = unparenthesize(&op.value) else {
                    return Err("config.pluginComponents must be an object literal".to_string());
                };
                plugin_components = Some(text(&wrapped, op.value.span()).to_string());
            }
        }
        if !is_plugin_components {
            let span = p.span();
            kept.push(wrapped[span.start as usize..span.end as usize].to_string());
        }
    }
    if plugin_components.is_none() {
        return Ok((None, Some(raw.to_string())));
    }
    if kept.is_empty() {
        return Ok((plugin_components, None));
    }
    Ok((plugin_components, Some(format!("{{ {} }}", kept.join(", ")))))
}

fn plugin_components_config_entries(raw: &str) -> Result<Vec<(String, String)>, String> {
    let wrapped = format!("({})", raw);
    let allocator = Allocator::default();
    let source_type = SourceType::default().with_typescript(true);
    let ret = Parser::new(&allocator, &wrapped, source_type).parse();
    if !ret.errors.is_empty() {
        return Err("config.pluginComponents must be an object literal".to_string());
    }
    let Some(Statement::ExpressionStatement(es)) = ret.program.body.first() else {
        return Err("config.pluginComponents must be an object literal".to_string());
    };
    let Expression::ObjectExpression(obj) = unparenthesize(&es.expression) else {
        return Err("config.pluginComponents must be an object literal".to_string());
    };
    let mut out = Vec::new();
    for p in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(op) = p else {
            return Err("config.pluginComponents cannot use spreads".to_string());
        };
        let key = op.key.static_name().ok_or("config.pluginComponents keys must be static")?.to_string();
        let Expression::StringLiteral(value) = unparenthesize(&op.value) else {
            return Err(format!(
                "config.pluginComponents['{}'] must be a string literal starting with 'plugin://'",
                key
            ));
        };
        let value = value.value.to_string();
        if !value.starts_with("plugin://") {
            return Err(format!(
                "M1015: config.pluginComponents['{}'] must start with 'plugin://', got '{}'",
                key, value
            ));
        }
        out.push((key, value));
    }
    Ok(out)
}

fn split_custom_tags_config(raw: &str) -> Result<(Option<String>, Option<String>), String> {
    let wrapped = format!("({})", raw);
    let allocator = Allocator::default();
    let source_type = SourceType::default().with_typescript(true);
    let ret = Parser::new(&allocator, &wrapped, source_type).parse();
    if !ret.errors.is_empty() {
        return Ok((None, Some(raw.to_string())));
    }
    let Some(Statement::ExpressionStatement(es)) = ret.program.body.first() else {
        return Ok((None, Some(raw.to_string())));
    };
    let Expression::ObjectExpression(obj) = unparenthesize(&es.expression) else {
        return Ok((None, Some(raw.to_string())));
    };
    let mut custom_tags = None;
    let mut kept: Vec<String> = Vec::new();
    for p in &obj.properties {
        let mut is_custom_tags = false;
        if let ObjectPropertyKind::ObjectProperty(op) = p {
            if op.key.static_name().as_deref() == Some("customTags") {
                is_custom_tags = true;
                let Expression::ArrayExpression(_) = unparenthesize(&op.value) else {
                    return Err("config.customTags must be an array literal".to_string());
                };
                custom_tags = Some(text(&wrapped, op.value.span()).to_string());
            }
        }
        if !is_custom_tags {
            let span = p.span();
            kept.push(wrapped[span.start as usize..span.end as usize].to_string());
        }
    }
    if custom_tags.is_none() {
        return Ok((None, Some(raw.to_string())));
    }
    if kept.is_empty() {
        return Ok((custom_tags, None));
    }
    Ok((custom_tags, Some(format!("{{ {} }}", kept.join(", ")))))
}

/// Splits `virtualHost`/`pureDataPattern`/`externalClasses` out of the config
/// object literal, leaving them out of what reaches `.json`/`build_json`.
fn split_component_options_config(raw: &str) -> Result<(Option<String>, Option<String>), String> {
    let wrapped = format!("({})", raw);
    let allocator = Allocator::default();
    let source_type = SourceType::default().with_typescript(true);
    let ret = Parser::new(&allocator, &wrapped, source_type).parse();
    if !ret.errors.is_empty() {
        return Ok((None, Some(raw.to_string())));
    }
    let Some(Statement::ExpressionStatement(es)) = ret.program.body.first() else {
        return Ok((None, Some(raw.to_string())));
    };
    let Expression::ObjectExpression(obj) = unparenthesize(&es.expression) else {
        return Ok((None, Some(raw.to_string())));
    };
    const KEYS: &[&str] = &["virtualHost", "pureDataPattern", "externalClasses"];
    let mut found: Vec<String> = Vec::new();
    let mut kept: Vec<String> = Vec::new();
    for p in &obj.properties {
        let mut is_match = false;
        if let ObjectPropertyKind::ObjectProperty(op) = p {
            if let Some(key) = op.key.static_name() {
                if KEYS.contains(&key.as_ref()) {
                    is_match = true;
                    found.push(text(&wrapped, p.span()).to_string());
                }
            }
        }
        if !is_match {
            let span = p.span();
            kept.push(wrapped[span.start as usize..span.end as usize].to_string());
        }
    }
    if found.is_empty() {
        return Ok((None, Some(raw.to_string())));
    }
    let component_options = Some(format!("{{ {} }}", found.join(", ")));
    if kept.is_empty() {
        return Ok((component_options, None));
    }
    Ok((component_options, Some(format!("{{ {} }}", kept.join(", ")))))
}

/// Component capability shorthand for the merged `options`/top-level fields.
#[derive(Default)]
struct ComponentOptions {
    virtual_host: Option<bool>,
    pure_data_pattern: Option<String>,
    external_classes: Vec<String>,
}

fn component_options_config_entries(raw: &str) -> Result<ComponentOptions, String> {
    let wrapped = format!("({})", raw);
    let allocator = Allocator::default();
    let source_type = SourceType::default().with_typescript(true);
    let ret = Parser::new(&allocator, &wrapped, source_type).parse();
    if !ret.errors.is_empty() {
        return Err("config.virtualHost/pureDataPattern/externalClasses must be static literals".to_string());
    }
    let Some(Statement::ExpressionStatement(es)) = ret.program.body.first() else {
        return Err("config.virtualHost/pureDataPattern/externalClasses must be static literals".to_string());
    };
    let Expression::ObjectExpression(obj) = unparenthesize(&es.expression) else {
        return Err("config.virtualHost/pureDataPattern/externalClasses must be static literals".to_string());
    };
    let mut out = ComponentOptions::default();
    for p in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(op) = p else { continue };
        let Some(key) = op.key.static_name() else { continue };
        match key.as_ref() {
            "virtualHost" => {
                let Expression::BooleanLiteral(b) = unparenthesize(&op.value) else {
                    return Err("config.virtualHost must be the literal true or false".to_string());
                };
                out.virtual_host = Some(b.value);
            }
            "pureDataPattern" => {
                let Expression::StringLiteral(s) = unparenthesize(&op.value) else {
                    return Err("config.pureDataPattern must be a string literal".to_string());
                };
                let pattern = s.value.to_string();
                if pattern.is_empty() {
                    return Err("config.pureDataPattern must not be empty".to_string());
                }
                if pattern.contains('/') || pattern.contains('\\') {
                    return Err(
                        "pureDataPattern must not contain '/' or '\\' — use simple prefixes like '^_'"
                            .to_string(),
                    );
                }
                out.pure_data_pattern = Some(pattern);
            }
            "externalClasses" => {
                let Expression::ArrayExpression(arr) = unparenthesize(&op.value) else {
                    return Err("config.externalClasses must be an array literal".to_string());
                };
                let is_class_charset = |s: &str| {
                    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
                };
                for el in &arr.elements {
                    let e = el
                        .as_expression()
                        .ok_or("config.externalClasses must hold string literals")?;
                    let Expression::StringLiteral(value) = unparenthesize(e) else {
                        return Err("config.externalClasses must hold string literals".to_string());
                    };
                    let value = value.value.to_string();
                    if !is_class_charset(&value) {
                        return Err(format!(
                            "config.externalClasses['{}'] must contain only letters, digits, '-' and '_'",
                            value
                        ));
                    }
                    out.external_classes.push(value);
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

fn custom_tags_config_entries(raw: &str) -> Result<Vec<String>, String> {
    let wrapped = format!("({})", raw);
    let allocator = Allocator::default();
    let source_type = SourceType::default().with_typescript(true);
    let ret = Parser::new(&allocator, &wrapped, source_type).parse();
    if !ret.errors.is_empty() {
        return Err("config.customTags must be an array literal".to_string());
    }
    let Some(Statement::ExpressionStatement(es)) = ret.program.body.first() else {
        return Err("config.customTags must be an array literal".to_string());
    };
    let Expression::ArrayExpression(arr) = unparenthesize(&es.expression) else {
        return Err("config.customTags must be an array literal".to_string());
    };
    let is_tag_charset = |s: &str| {
        !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    };
    let mut out = Vec::new();
    for el in &arr.elements {
        let e = el.as_expression().ok_or("config.customTags must hold string literals")?;
        let Expression::StringLiteral(value) = unparenthesize(e) else {
            return Err("config.customTags must hold string literals".to_string());
        };
        let value = value.value.to_string();
        if !is_tag_charset(&value) {
            return Err(format!(
                "config.customTags['{}'] must contain only letters, digits, '-' and '_'",
                value
            ));
        }
        out.push(value);
    }
    Ok(out)
}

fn events_config_entries(raw: &str) -> Result<Vec<(String, String)>, String> {
    let wrapped = format!("({})", raw);
    let allocator = Allocator::default();
    let source_type = SourceType::default().with_typescript(true);
    let ret = Parser::new(&allocator, &wrapped, source_type).parse();
    if !ret.errors.is_empty() {
        return Err("config.events must be an object literal".to_string());
    }
    let Some(Statement::ExpressionStatement(es)) = ret.program.body.first() else {
        return Err("config.events must be an object literal".to_string());
    };
    let Expression::ObjectExpression(obj) = unparenthesize(&es.expression) else {
        return Err("config.events must be an object literal".to_string());
    };
    let mut out = Vec::new();
    for p in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(op) = p else {
            return Err("config.events cannot use spreads".to_string());
        };
        let key = op.key.static_name().ok_or("config.events keys must be static")?.to_string();
        out.push((key, text(&wrapped, op.value.span()).to_string()));
    }
    Ok(out)
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

/// `onToggle` → `toggle`
fn derive_order(deriveds: &[DerivedDecl]) -> Vec<usize> {
    let n = deriveds.len();
    let reads = |arrow: &str, name: &str| -> bool {
        let needle = format!("this.data.{}", name);
        for (i, _) in arrow.match_indices(&needle) {
            let after = arrow[i + needle.len()..].chars().next();
            if !after.map(|c| c.is_alphanumeric() || c == '_' || c == '$').unwrap_or(false) {
                return true;
            }
        }
        false
    };
    let mut indegree = vec![0usize; n];
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 0..n {
        for j in 0..n {
            if i != j && reads(&deriveds[i].arrow, &deriveds[j].name) {
                adj[j].push(i);
                indegree[i] += 1;
            }
        }
    }
    let mut order = Vec::with_capacity(n);
    let mut ready: Vec<usize> = (0..n).filter(|&i| indegree[i] == 0).collect();
    while let Some(&i) = ready.first() {
        ready.remove(0);
        order.push(i);
        for &next in &adj[i] {
            indegree[next] -= 1;
            if indegree[next] == 0 {
                let pos = ready.iter().position(|&r| r > next).unwrap_or(ready.len());
                ready.insert(pos, next);
            }
        }
    }
    for i in 0..n {
        if !order.contains(&i) {
            order.push(i);
        }
    }
    order
}

fn store_deps(code: &str) -> Option<std::collections::BTreeSet<String>> {
    let ident = |s: &str| -> String {
        s.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$').collect()
    };
    let mut deps = std::collections::BTreeSet::new();
    let mut rest = code;
    while let Some(pos) = rest.find("__S") {
        let after = &rest[pos + 3..];
        let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        let after_digits = &after[digits.len()..];
        let Some(tail) = after_digits.strip_prefix('.') else {
            return None;
        };
        let name = ident(tail);
        if digits.is_empty() || name.is_empty() || !tail[name.len()..].starts_with(".value") {
            return None;
        }
        deps.insert(name);
        rest = &rest[pos + 3..];
    }
    Some(deps)
}

fn derived_deps(
    arrow: &str,
    pure_methods: &[(String, std::collections::BTreeSet<String>)],
) -> Option<Vec<String>> {
    let ident = |s: &str| -> String {
        s.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$').collect()
    };
    let mut deps = std::collections::BTreeSet::new();
    let mut rest = arrow;
    while let Some(pos) = rest.find("this.") {
        let after = &rest[pos + 5..];
        if let Some(tail) = after.strip_prefix("data.") {
            let name = ident(tail);
            if name.is_empty() {
                return None;
            }
            deps.insert(name);
        } else if let Some(tail) = after.strip_prefix('_') {
            let name = ident(tail);
            if name.is_empty() {
                return None;
            }
            deps.insert(name);
        } else {
            let name = ident(after);
            let after_name = after[name.len()..].trim_start();
            if name.is_empty() || !after_name.starts_with('(') {
                return None;
            }
            match pure_methods.iter().find(|(m, _)| *m == name) {
                Some((_, method_deps)) => deps.extend(method_deps.iter().cloned()),
                None => return None,
            }
        }
        rest = &rest[pos + 5..];
    }
    deps.extend(store_deps(arrow)?);
    Some(deps.into_iter().collect())
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
    const_seeds: &[String],
) -> String {
    let mut out = String::new();
    out.push_str("// generated by mistc — do not edit\n");
    out.push_str(&format!("const rt = require('{}');\n", rt_require));
    let uses_mistq = analysis.methods.iter().any(|m| m.body.contains("__mistq("))
        || analysis.lifecycles.iter().any(|l| l.body.contains("__mistq("))
        || analysis.deriveds.iter().any(|d| d.arrow.contains("__mistq("));
    if uses_mistq {
        out.push_str(
            "function __mistq(p) { const s = Object.keys(p).map(k => encodeURIComponent(k) + '=' + encodeURIComponent(p[k])).join('&'); return s ? '?' + s : ''; }\n",
        );
    }
    for si in &analysis.store_imports {
        out.push_str(&format!("const {} = require('{}');\n", si.alias, si.require_path));
    }
    for pi in &analysis.plugin_imports {
        out.push_str(&format!("const {} = requirePlugin('{}');\n", pi.local, pi.plugin));
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

    let pure_methods: Vec<(String, std::collections::BTreeSet<String>)> = analysis
        .methods
        .iter()
        .filter(|m| !m.body.contains("this."))
        .filter_map(|m| store_deps(&m.body).map(|d| (m.name.clone(), d)))
        .collect();
    body.push_str(&format!("{}__derive() {{\n{}  const __o = {{}};\n", pad, pad));
    for i in derive_order(&analysis.deriveds) {
        let d = &analysis.deriveds[i];
        let key = match derived_keys.get(i).and_then(|k| k.as_deref()) {
            Some(k) => format!("'{}'", k),
            None => "null".to_string(),
        };
        let deps = match derived_deps(&d.arrow, &pure_methods) {
            Some(names) => format!(
                "[{}]",
                names.iter().map(|n| format!("'{}'", n)).collect::<Vec<_>>().join(", ")
            ),
            None => "null".to_string(),
        };
        body.push_str(&format!(
            "{}  rt.derive(this, __o, '{}', {}, {}, {});\n",
            pad, d.name, key, d.arrow, deps
        ));
    }
    body.push_str(&format!("{}  return __o;\n{}}},\n", pad, pad));

    body.push_str(&format!("{}__set(path, value) {{\n{}  rt.set(this, path, value);\n{}}},\n", pad, pad, pad));

    for m in &analysis.methods {
        let prefix = if m.is_async { "async " } else { "" };
        body.push_str(&format!("{}{}{}{} {},\n", pad, prefix, m.name, m.params, m.body));
    }

    for cb in &analysis.callback_props {
        let options = analysis.event_options.iter().find(|(name, _)| name == cb).map(|(_, opts)| opts);
        match options {
            Some(opts) => body.push_str(&format!(
                "{}{}(...args) {{\n{}  this.triggerEvent('{}', {{ args }}, {});\n{}}},\n",
                pad, cb, pad, event_name(cb), opts, pad
            )),
            None => body.push_str(&format!(
                "{}{}(...args) {{\n{}  this.triggerEvent('{}', {{ args }});\n{}}},\n",
                pad, cb, pad, event_name(cb), pad
            )),
        }
    }

    for vb in vbinds {
        // model:value already rendered the keystroke; sync the logic-side mirror
        // and recompute deriveds through the normal batch
        body.push_str(&format!(
            "{}__vb_{}(e) {{\n{}  this.data.{} = e.detail.value;\n{}  rt.touch(this, '{}');\n{}}},\n",
            pad, vb, pad, vb, pad, vb, pad
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
        emit_data(&mut out, analysis, "  ", const_seeds);
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
    let mut options: Vec<String> = Vec::new();
    if multiple_slots {
        options.push("multipleSlots: true".to_string());
    }
    if analysis.virtual_host == Some(true) {
        options.push("virtualHost: true".to_string());
    }
    if let Some(pattern) = &analysis.pure_data_pattern {
        options.push(format!("pureDataPattern: /{}/", pattern));
    }
    if !options.is_empty() {
        out.push_str(&format!("  options: {{ {} }},\n", options.join(", ")));
    }
    if !analysis.external_classes.is_empty() {
        let classes: Vec<String> =
            analysis.external_classes.iter().map(|c| format!("\"{}\"", c)).collect();
        out.push_str(&format!("  externalClasses: [{}],\n", classes.join(", ")));
    }
    out.push_str("  properties: {\n");
    for p in &analysis.data_props {
        let observer = if analysis.deriveds.is_empty() {
            String::new()
        } else {
            format!(", observer() {{ rt.touch(this, '{}'); }}", p.name)
        };
        let ty = p.prop_type.as_wx();
        match &p.default {
            Some(d) => out.push_str(&format!(
                "    {}: {{ type: {}, value: {}{} }},\n",
                p.name, ty, d, observer
            )),
            None => out.push_str(&format!("    {}: {{ type: {}{} }},\n", p.name, ty, observer)),
        }
    }
    out.push_str("  },\n");
    emit_data(&mut out, analysis, "  ", const_seeds);
    out.push_str("  methods: {\n");
    out.push_str(&body);
    out.push_str("  },\n");

    out.push_str("  lifetimes: {\n");
    let mut has_attached = false;
    let mut has_detached = false;
    let page_lifetime = |hook: &str| PAGE_LIFETIME_HOOKS.iter().find(|(h, _)| *h == hook);
    for l in &analysis.lifecycles {
        if page_lifetime(&l.hook).is_some() {
            continue;
        }
        let prefix = if l.is_async { "async " } else { "" };
        let hook = match l.hook.as_str() {
            "onAttach" => "attached",
            "onDetach" => "detached",
            "onReady" => "ready",
            "onCreate" => "created",
            "onMove" => "moved",
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

    let page_lifetimes: Vec<&Lifecycle> =
        analysis.lifecycles.iter().filter(|l| page_lifetime(&l.hook).is_some()).collect();
    if !page_lifetimes.is_empty() {
        out.push_str("  pageLifetimes: {\n");
        for l in page_lifetimes {
            let prefix = if l.is_async { "async " } else { "" };
            let (_, mapped) = page_lifetime(&l.hook).unwrap();
            out.push_str(&format!("    {}{}{} {},\n", prefix, mapped, l.params, l.body));
        }
        out.push_str("  },\n");
    }
    out.push_str("});\n");
    out
}

/// Compile a `stores/*.ts` module: `store(init)` exports become runtime store
/// boxes; exported functions get their mutations compiled to path-precise
/// `__set` calls; everything is wired up as a plain CommonJS module.
pub fn compile_store_module(src: &str, rt_require: &str) -> Result<(String, StoreModuleInfo), String> {
    let stripped = strip_types(src, 1)?;
    let src = stripped.src.as_str();
    let allocator = Allocator::default();
    let source_type = SourceType::default().with_typescript(true);
    let ret = Parser::new(&allocator, src, source_type).parse();
    if !ret.errors.is_empty() {
        let msgs: Vec<String> = ret.errors.iter().map(|e| e.to_string()).collect();
        return Err(format!("store parse errors: {}", msgs.join("; ")));
    }
    let program = ret.program;

    let mut info = StoreModuleInfo::default();
    // first pass: find store names so mutations anywhere in the module compile
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

    // inside the module the accessor is the store binding itself
    let accessors: std::collections::BTreeMap<String, String> =
        info.stores.iter().map(|s| (s.clone(), s.clone())).collect();
    let mut alias_scan = AliasScan::new(src, info.stores.clone(), info.stores.clone(), Vec::new());
    alias_scan.visit_program(&program);
    let value_errors = alias_scan.value_errors(1);
    if !value_errors.is_empty() {
        return Err(value_errors.join("; "));
    }
    let aliases = alias_scan.finish();

    let mut collector = MutationCollector {
        src,
        states: Vec::new(),
        unbound: Vec::new(),
        stores: accessors,
        aliases,
        prop_rewrites: Vec::new(),
        edits: Vec::new(),
        errors: Vec::new(),
        line_offset: 1,
        route_refs: Vec::new(),
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
                if let Some(specs) = import.specifiers.as_ref() {
                    check_mist_specifiers(src, specs, 1)?;
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
                            let args = match unparenthesize(init) {
                                Expression::CallExpression(call) => {
                                    let parts: Vec<String> =
                                        call.arguments.iter().map(|a| apply(a.span())).collect();
                                    if parts.is_empty() {
                                        "null".to_string()
                                    } else {
                                        parts.join(", ")
                                    }
                                }
                                _ => "null".to_string(),
                            };
                            out.push_str(&format!("const {} = rt.store({});\n", name, args));
                        } else if matches!(
                            unparenthesize(init),
                            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
                        ) {
                            // page-side wrappers call these — only functions qualify
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

/// Only the exports of a store module — used to classify a page's imports.
pub fn store_module_info(src: &str) -> Result<StoreModuleInfo, String> {
    compile_store_module(src, "./mist-rt.js").map(|(_, info)| info)
}

/// Convert an `export const config = {...}` object literal into JSON, properly —
/// quote-aware via the real parser, not regex.
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

/// Top-level property names of a config object literal — used to detect
/// collisions with compiler-generated JSON fields (M1014).
pub fn config_top_level_keys(config: &str) -> Result<Vec<String>, String> {
    let allocator = Allocator::default();
    let src = format!("const __c = {};", config);
    let ret = Parser::new(&allocator, &src, SourceType::default().with_typescript(true)).parse();
    if !ret.errors.is_empty() {
        return Err("config must be a static object literal".to_string());
    }
    for stmt in &ret.program.body {
        if let Statement::VariableDeclaration(var) = stmt {
            if let Some(init) = &var.declarations[0].init {
                let Expression::ObjectExpression(obj) = unparenthesize(init) else {
                    return Err("config must be a static object literal".to_string());
                };
                let mut keys = Vec::new();
                for prop in &obj.properties {
                    let ObjectPropertyKind::ObjectProperty(p) = prop else {
                        return Err("config objects cannot use spreads".to_string());
                    };
                    keys.push(p.key.static_name().ok_or("config keys must be static")?.to_string());
                }
                return Ok(keys);
            }
        }
    }
    Err("config must be a static object literal".to_string())
}

/// Keys of a manual `config.usingComponents` object literal — tags the user
/// hand-registered (e.g. third-party npm components) that M1019 must not
/// flag as unknown.
pub fn config_using_components_keys(config: &str) -> Result<Vec<String>, String> {
    let allocator = Allocator::default();
    let src = format!("const __c = {};", config);
    let ret = Parser::new(&allocator, &src, SourceType::default().with_typescript(true)).parse();
    if !ret.errors.is_empty() {
        return Ok(Vec::new());
    }
    for stmt in &ret.program.body {
        if let Statement::VariableDeclaration(var) = stmt {
            if let Some(init) = &var.declarations[0].init {
                let Expression::ObjectExpression(obj) = unparenthesize(init) else {
                    return Ok(Vec::new());
                };
                for prop in &obj.properties {
                    let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
                    if p.key.static_name().as_deref() != Some("usingComponents") {
                        continue;
                    }
                    let Expression::ObjectExpression(using) = unparenthesize(&p.value) else {
                        return Ok(Vec::new());
                    };
                    let mut keys = Vec::new();
                    for up in &using.properties {
                        if let ObjectPropertyKind::ObjectProperty(uop) = up {
                            if let Some(k) = uop.key.static_name() {
                                keys.push(k.to_string());
                            }
                        }
                    }
                    return Ok(keys);
                }
            }
        }
    }
    Ok(Vec::new())
}

/// Does `config.tabBar.custom` equal `true`? Used to wire the custom
/// tab bar contract (M1020): file presence must match this flag.
pub fn config_tab_bar_custom(config: &str) -> bool {
    let allocator = Allocator::default();
    let src = format!("const __c = {};", config);
    let ret = Parser::new(&allocator, &src, SourceType::default().with_typescript(true)).parse();
    if !ret.errors.is_empty() {
        return false;
    }
    for stmt in &ret.program.body {
        let Statement::VariableDeclaration(var) = stmt else { continue };
        let Some(init) = &var.declarations[0].init else { continue };
        let Expression::ObjectExpression(obj) = unparenthesize(init) else { return false };
        for prop in &obj.properties {
            let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
            if p.key.static_name().as_deref() != Some("tabBar") {
                continue;
            }
            let Expression::ObjectExpression(tab_bar) = unparenthesize(&p.value) else {
                return false;
            };
            for tp in &tab_bar.properties {
                let ObjectPropertyKind::ObjectProperty(tprop) = tp else { continue };
                if tprop.key.static_name().as_deref() != Some("custom") {
                    continue;
                }
                return matches!(
                    unparenthesize(&tprop.value),
                    Expression::BooleanLiteral(b) if b.value
                );
            }
            return false;
        }
    }
    false
}

/// `config.tabBar.list[].pagePath` when every entry is a static string —
/// `None` when `tabBar`/`list` is absent or any entry isn't statically
/// extractable (M1021's `navigate.switchTab` check then falls back to plain
/// route validation instead of the tab-bar-specific message).
pub fn config_tab_bar_page_paths(config: &str) -> Option<Vec<String>> {
    let allocator = Allocator::default();
    let src = format!("const __c = {};", config);
    let ret = Parser::new(&allocator, &src, SourceType::default().with_typescript(true)).parse();
    if !ret.errors.is_empty() {
        return None;
    }
    for stmt in &ret.program.body {
        let Statement::VariableDeclaration(var) = stmt else { continue };
        let Some(init) = &var.declarations[0].init else { continue };
        let Expression::ObjectExpression(obj) = unparenthesize(init) else { return None };
        for prop in &obj.properties {
            let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
            if p.key.static_name().as_deref() != Some("tabBar") {
                continue;
            }
            let Expression::ObjectExpression(tab_bar) = unparenthesize(&p.value) else { return None };
            for tp in &tab_bar.properties {
                let ObjectPropertyKind::ObjectProperty(tprop) = tp else { continue };
                if tprop.key.static_name().as_deref() != Some("list") {
                    continue;
                }
                let Expression::ArrayExpression(list) = unparenthesize(&tprop.value) else {
                    return None;
                };
                let mut paths = Vec::new();
                for el in &list.elements {
                    let ArrayExpressionElement::ObjectExpression(item) = el else { return None };
                    let mut found = None;
                    for ip in &item.properties {
                        let ObjectPropertyKind::ObjectProperty(iprop) = ip else { continue };
                        if iprop.key.static_name().as_deref() != Some("pagePath") {
                            continue;
                        }
                        let Expression::StringLiteral(s) = unparenthesize(&iprop.value) else {
                            return None;
                        };
                        found = Some(s.value.to_string());
                    }
                    match found {
                        Some(path) => paths.push(path),
                        None => return None,
                    }
                }
                return Some(paths);
            }
            return None;
        }
    }
    None
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

/// Build generated deriveds for template-hoisted expressions. Returns
/// (derived, wx:key) pairs to append to the analysis, with reads rewritten the
/// same way frontmatter code is (bound → this.data, unbound → this._x, stores
/// → alias, method calls → this.*).
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

/// `app.mist` → `app.js`: App({ onLaunch/onShow/onHide + methods }).
/// Installs the performance observer before App() so launch entries are captured;
/// `__perf` exposes them via getApp() for tooling (benchmark/devtools/measure.js).
pub fn emit_app_js(analysis: &Analysis) -> String {
    let mut out = String::new();
    out.push_str("// generated by mistc — do not edit\n");
    out.push_str("const rt = require('./mist-rt.js');\n");
    out.push_str("rt.observePerf();\n");
    let uses_mistq = analysis.methods.iter().any(|m| m.body.contains("__mistq("))
        || analysis.lifecycles.iter().any(|l| l.body.contains("__mistq("));
    if uses_mistq {
        out.push_str(
            "function __mistq(p) { const s = Object.keys(p).map(k => encodeURIComponent(k) + '=' + encodeURIComponent(p[k])).join('&'); return s ? '?' + s : ''; }\n",
        );
    }
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

fn emit_data(out: &mut String, analysis: &Analysis, pad: &str, const_seeds: &[String]) {
    out.push_str(&format!("{}data: {{\n", pad));
    for name in const_seeds {
        out.push_str(&format!("{}  {}: {},\n", pad, name, name));
    }
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
    /// (`\buser\.value` regex, replacement `__S0.user.value`)
    store_reads: Vec<(Regex, String)>,
    /// unbound state reads → `this._name`
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
        // `(^|[^.\w$])` guard: `x.send()` is a member call on `x`, not the
        // frontmatter method `send` — only bare calls get the `this.` prefix
        let calls = if methods.is_empty() {
            None
        } else {
            Some(
                Regex::new(&format!(r"(^|[^.\w$])({})\s*\(", methods.join("|")))
                    .map_err(|e| e.to_string())?,
            )
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
            Some(re) => re.replace_all(&out, "${1}this.$2(").to_string(),
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

#[derive(Clone)]
struct BindingScope {
    name: String,
    start: u32,
    end: u32,
    source: Option<String>,
}

struct AliasScan<'s> {
    src: &'s str,
    roots: Vec<String>,
    value_roots: Vec<String>,
    prop_roots: Vec<String>,
    bindings: Vec<BindingScope>,
    reassigned: Vec<(String, u32)>,
    value_ok: std::collections::BTreeSet<u32>,
    value_refs: Vec<(String, u32)>,
    prop_refs: Vec<(String, u32)>,
    shorthand_starts: std::collections::BTreeSet<u32>,
    fn_stack: Vec<(u32, u32)>,
}

impl AliasScan<'_> {
    fn new<'s>(
        src: &'s str,
        roots: Vec<String>,
        value_roots: Vec<String>,
        prop_roots: Vec<String>,
    ) -> AliasScan<'s> {
        AliasScan {
            src,
            roots,
            value_roots,
            prop_roots,
            bindings: Vec::new(),
            reassigned: Vec::new(),
            value_ok: std::collections::BTreeSet::new(),
            value_refs: Vec::new(),
            prop_refs: Vec::new(),
            shorthand_starts: std::collections::BTreeSet::new(),
            fn_stack: Vec::new(),
        }
    }

    fn shadowed(&self, name: &str, pos: u32) -> bool {
        self.bindings.iter().any(|b| b.name == *name && b.start <= pos && pos < b.end)
    }

    fn prop_edits(&self) -> Vec<Edit> {
        let mut out = Vec::new();
        for (name, start) in &self.prop_refs {
            if self.shadowed(name, *start) {
                continue;
            }
            let text = if self.shorthand_starts.contains(start) {
                format!("{}: this.data.{}", name, name)
            } else {
                format!("this.data.{}", name)
            };
            out.push(Edit { start: *start, end: *start + name.len() as u32, text });
        }
        out
    }

    fn enclosing_end(&self) -> u32 {
        self.fn_stack.last().map(|&(_, e)| e).unwrap_or(self.src.len() as u32)
    }

    fn shadow_params(&mut self, params: &FormalParameters, span: Span) {
        for p in &params.items {
            let mut names = Vec::new();
            collect_binding_idents(&p.pattern.kind, &mut names);
            for name in names {
                self.bindings.push(BindingScope {
                    name,
                    start: span.start,
                    end: span.end,
                    source: None,
                });
            }
        }
    }

    fn value_errors(&self, line_offset: usize) -> Vec<String> {
        let mut out = Vec::new();
        for (name, start) in &self.value_refs {
            if self.value_ok.contains(start) {
                continue;
            }
            if self.shadowed(name, *start) {
                continue;
            }
            let (line, col) = line_col(self.src, *start as usize, line_offset);
            out.push(format!(
                "M1007 at line {}:{}: `{}` is reactive — access it as `{}.value`\n  help: reads and writes must go through `.value` so the compiler can emit setData",
                line, col, name, name
            ));
        }
        out
    }

    fn finish(self) -> Vec<BindingScope> {
        let reassigned = self.reassigned;
        self.bindings
            .into_iter()
            .map(|mut b| {
                if b.source.is_some()
                    && reassigned.iter().any(|(n, p)| *n == b.name && b.start <= *p && *p < b.end)
                {
                    b.source = None;
                }
                b
            })
            .collect()
    }
}

fn collect_binding_idents(kind: &BindingPatternKind, out: &mut Vec<String>) {
    match kind {
        BindingPatternKind::BindingIdentifier(id) => out.push(id.name.to_string()),
        BindingPatternKind::ObjectPattern(pat) => {
            for p in &pat.properties {
                collect_binding_idents(&p.value.kind, out);
            }
            if let Some(rest) = &pat.rest {
                collect_binding_idents(&rest.argument.kind, out);
            }
        }
        BindingPatternKind::ArrayPattern(pat) => {
            for e in pat.elements.iter().flatten() {
                collect_binding_idents(&e.kind, out);
            }
            if let Some(rest) = &pat.rest {
                collect_binding_idents(&rest.argument.kind, out);
            }
        }
        BindingPatternKind::AssignmentPattern(pat) => {
            collect_binding_idents(&pat.left.kind, out);
        }
    }
}

fn alias_source(init_text: &str, roots: &[String]) -> Option<String> {
    let idx = init_text.find(".value")?;
    let root = &init_text[..idx];
    if root.is_empty()
        || !root.chars().all(|c| c.is_alphanumeric() || c == '_')
        || !roots.iter().any(|r| r == root)
    {
        return None;
    }
    let rest = &init_text[idx + 6..];
    if !rest.chars().all(|c| c.is_alphanumeric() || "._[]$'\"".contains(c)) {
        return None;
    }
    Some(init_text.to_string())
}

impl<'a> Visit<'a> for AliasScan<'_> {
    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        let top_level = self.fn_stack.is_empty();
        let end = self.enclosing_end();
        let source = match &it.id.kind {
            BindingPatternKind::BindingIdentifier(_) => it
                .init
                .as_ref()
                .and_then(|init| alias_source(text(self.src, init.span()), &self.roots)),
            _ => None,
        };
        let mut names = Vec::new();
        collect_binding_idents(&it.id.kind, &mut names);
        for name in names {
            if top_level
                && (self.roots.iter().any(|r| *r == name)
                    || self.value_roots.iter().any(|r| *r == name)
                    || self.prop_roots.iter().any(|r| *r == name))
            {
                continue;
            }
            self.bindings.push(BindingScope {
                name,
                start: it.span.end,
                end,
                source: source.clone(),
            });
        }
        walk::walk_variable_declarator(self, it);
    }

    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        self.shadow_params(&it.params, it.span);
        self.fn_stack.push((it.span.start, it.span.end));
        walk::walk_function(self, it, flags);
        self.fn_stack.pop();
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        self.shadow_params(&it.params, it.span);
        self.fn_stack.push((it.span.start, it.span.end));
        walk::walk_arrow_function_expression(self, it);
        self.fn_stack.pop();
    }

    fn visit_catch_clause(&mut self, it: &CatchClause<'a>) {
        if let Some(param) = &it.param {
            let mut names = Vec::new();
            collect_binding_idents(&param.pattern.kind, &mut names);
            for name in names {
                self.bindings.push(BindingScope {
                    name,
                    start: it.span.start,
                    end: it.span.end,
                    source: None,
                });
            }
        }
        walk::walk_catch_clause(self, it);
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        let left = text(self.src, it.left.span());
        if !left.is_empty() && left.chars().all(|c| c.is_alphanumeric() || c == '_') {
            self.reassigned.push((left.to_string(), it.span.start));
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_member_expression(&mut self, it: &MemberExpression<'a>) {
        if it.static_property_name() == Some("value") {
            if let Expression::Identifier(id) = it.object() {
                self.value_ok.insert(id.span.start);
            }
        }
        walk::walk_member_expression(self, it);
    }

    fn visit_for_of_statement(&mut self, it: &ForOfStatement<'a>) {
        if let ForStatementLeft::VariableDeclaration(decl) = &it.left {
            if let Some(first) = decl.declarations.first() {
                if let BindingPatternKind::BindingIdentifier(id) = &first.id.kind {
                    if let Some(source) = alias_source(text(self.src, it.right.span()), &self.roots)
                    {
                        self.bindings.push(BindingScope {
                            name: id.name.to_string(),
                            start: first.span.end,
                            end: it.span.end,
                            source: Some(format!("{}[…]", source)),
                        });
                    }
                }
            }
        }
        walk::walk_for_of_statement(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Some(member) = it.callee.as_member_expression() {
            let is_iter = member
                .static_property_name()
                .is_some_and(|m| {
                    ["forEach", "map", "filter", "find", "some", "every", "flatMap"].contains(&m)
                });
            if is_iter {
                if let Some(source) =
                    alias_source(text(self.src, member.object().span()), &self.roots)
                {
                    let param = it.arguments.first().and_then(|a| a.as_expression()).and_then(
                        |e| match e {
                            Expression::ArrowFunctionExpression(a) => {
                                a.params.items.first().map(|p| (p, a.span))
                            }
                            Expression::FunctionExpression(f) => {
                                f.params.items.first().map(|p| (p, f.span))
                            }
                            _ => None,
                        },
                    );
                    if let Some((p, span)) = param {
                        if let BindingPatternKind::BindingIdentifier(id) = &p.pattern.kind {
                            self.bindings.push(BindingScope {
                                name: id.name.to_string(),
                                start: span.start,
                                end: span.end,
                                source: Some(format!("{}[…]", source)),
                            });
                        }
                    }
                }
            }
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        let name = it.name.to_string();
        if self.value_roots.iter().any(|r| *r == name) {
            self.value_refs.push((name, it.span.start));
        } else if self.prop_roots.iter().any(|r| *r == name) {
            self.prop_refs.push((name, it.span.start));
        }
    }

    fn visit_object_property(&mut self, it: &ObjectProperty<'a>) {
        if it.shorthand {
            self.shorthand_starts.insert(it.value.span().start);
        }
        walk::walk_object_property(self, it);
    }
}

struct MutationCollector<'s> {
    src: &'s str,
    states: Vec<String>,
    /// unbound states: mutations apply to `this._x` locally + rt.touch (no setData path)
    unbound: Vec<String>,
    /// store root → accessor (`user` → `__S0.user` in pages, `user` → `user` in store modules)
    stores: std::collections::BTreeMap<String, String>,
    aliases: Vec<BindingScope>,
    prop_rewrites: Vec<Edit>,
    edits: Vec<Edit>,
    errors: Vec<String>,
    line_offset: usize,
    route_refs: Vec<RouteRef>,
}

impl<'s> MutationCollector<'s> {
    fn slice_with_props(&self, span: Span) -> String {
        let mut s = text(self.src, span).to_string();
        let mut local: Vec<&Edit> = self
            .prop_rewrites
            .iter()
            .filter(|e| e.start >= span.start && e.end <= span.end)
            .collect();
        local.sort_by(|a, b| b.start.cmp(&a.start));
        for e in local {
            let a = (e.start - span.start) as usize;
            let b = (e.end - span.start) as usize;
            s.replace_range(a..b, &e.text);
        }
        s
    }

    fn state_path(&self, target_text: &str) -> Option<(String, String)> {
        let idx = target_text.find(".value")?;
        let root = &target_text[..idx];
        if !self.states.iter().any(|s| s == root) {
            return None;
        }
        let rest = &target_text[idx + 6..];
        Some((root.to_string(), rest.to_string()))
    }

    /// local-mutation rewrite for unbound states: `todos.value[i].x = v` →
    /// `(this._todos[i].x = v, rt.touch(this))`
    fn unbound_edit(&self, span: Span) -> Option<Edit> {
        let t = self.slice_with_props(span);
        let t = t.as_str();
        let idx = t.find(".value")?;
        let root = &t[..idx];
        let root_ident: String =
            root.chars().rev().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        let root_ident: String = root_ident.chars().rev().collect();
        if !self.unbound.iter().any(|u| *u == root_ident) {
            return None;
        }
        let mut replaced = t.to_string();
        for u in &self.unbound {
            replaced = replaced.replace(&format!("{}.value", u), &format!("this._{}", u));
        }
        Some(Edit {
            start: span.start,
            end: span.end,
            text: format!(";({}, rt.touch(this, '{}'))", replaced, root_ident),
        })
    }

    fn alias_target(&self, target_text: &str, pos: u32) -> Option<(String, String, String)> {
        let root: String =
            target_text.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if root.is_empty() {
            return None;
        }
        let mut best: Option<&BindingScope> = None;
        for b in &self.aliases {
            if b.name != root || pos < b.start || pos >= b.end {
                continue;
            }
            let better = match best {
                None => true,
                Some(cur) => {
                    b.start > cur.start
                        || (b.start == cur.start && b.source.is_some() && cur.source.is_none())
                }
            };
            if better {
                best = Some(b);
            }
        }
        let source = best?.source.clone()?;
        let suffix = target_text[root.len()..].to_string();
        Some((root, source, suffix))
    }

    fn push_m1001(&mut self, span: Span, site: &str, root: &str, source: &str, fix: &str) {
        let (line, col) = line_col(self.src, span.start as usize, self.line_offset);
        self.errors.push(format!(
            "M1001 at line {}:{}: `{}` mutates state through `{}`, an alias of `{}` — aliased writes don't compile to setData\n  help: write through the state path instead: `{}`",
            line, col, site, root, source, fix
        ));
    }

    /// (accessor, rest-after-.value) when the target is a store write.
    fn store_path(&self, target_text: &str) -> Option<(String, String)> {
        let idx = target_text.find(".value")?;
        let root = &target_text[..idx];
        let acc = self.stores.get(root)?;
        let rest = &target_text[idx + 6..];
        Some((acc.clone(), rest.to_string()))
    }

    /// route argument must be a string literal so it can be checked against the
    /// compiled page list — identifiers, template interpolation and concatenation
    /// all error, because the compiler cannot see the value at compile time.
    fn route_literal(&mut self, arg: &Argument, call_span: Span) -> Option<String> {
        let expr = arg.as_expression()?;
        match unparenthesize(expr) {
            Expression::StringLiteral(s) => Some(s.value.to_string()),
            Expression::TemplateLiteral(t) if t.expressions.is_empty() => {
                Some(t.quasis.first().map(|q| q.value.raw.to_string()).unwrap_or_default())
            }
            _ => {
                let (line, col) = line_col(self.src, call_span.start as usize, self.line_offset);
                self.errors.push(format!(
                    "M1021 at line {}:{}: navigate routes must be literal strings so they can be checked against the page list",
                    line, col
                ));
                None
            }
        }
    }

    /// Detects `navigate(route, params?)` and `navigate.replace/back/switchTab(...)`,
    /// pushing a rewrite `Edit` and (for `push`/`replace`/`switchTab`) a `RouteRef`
    /// for late project-level validation. Returns `true` if the call was a navigate
    /// call (handled, whether or not it produced an edit — e.g. a literal error).
    fn try_navigate_call(&mut self, it: &CallExpression<'_>) -> bool {
        match &it.callee {
            Expression::Identifier(id) if id.name == "navigate" => {
                let Some(route_arg) = it.arguments.first() else {
                    let (line, col) = line_col(self.src, it.span.start as usize, self.line_offset);
                    self.errors.push(format!(
                        "M1021 at line {}:{}: navigate(route) requires a literal route string",
                        line, col
                    ));
                    return true;
                };
                let Some(route) = self.route_literal(route_arg, it.span) else { return true };
                let (line, col) = line_col(self.src, it.span.start as usize, self.line_offset);
                self.route_refs.push(RouteRef { route: route.clone(), kind: RouteRefKind::Push, line, col });
                let url = match it.arguments.get(1) {
                    Some(params) => {
                        let params_text = self.slice_with_props(params.span());
                        format!("'{}' + __mistq({})", route, params_text)
                    }
                    None => format!("'{}'", route),
                };
                let replacement = format!("wx.navigateTo({{ url: {} }})", url);
                self.edits.push(Edit { start: it.span.start, end: it.span.end, text: replacement });
                true
            }
            Expression::StaticMemberExpression(member) => {
                let Expression::Identifier(id) = &member.object else { return false };
                if id.name != "navigate" {
                    return false;
                }
                match member.property.name.as_str() {
                    "replace" => {
                        let Some(route_arg) = it.arguments.first() else {
                            let (line, col) = line_col(self.src, it.span.start as usize, self.line_offset);
                            self.errors.push(format!(
                                "M1021 at line {}:{}: navigate.replace(route) requires a literal route string",
                                line, col
                            ));
                            return true;
                        };
                        let Some(route) = self.route_literal(route_arg, it.span) else { return true };
                        let (line, col) = line_col(self.src, it.span.start as usize, self.line_offset);
                        self.route_refs.push(RouteRef {
                            route: route.clone(),
                            kind: RouteRefKind::Replace,
                            line,
                            col,
                        });
                        let url = match it.arguments.get(1) {
                            Some(params) => {
                                let params_text = self.slice_with_props(params.span());
                                format!("'{}' + __mistq({})", route, params_text)
                            }
                            None => format!("'{}'", route),
                        };
                        let replacement = format!("wx.redirectTo({{ url: {} }})", url);
                        self.edits.push(Edit { start: it.span.start, end: it.span.end, text: replacement });
                        true
                    }
                    "back" => {
                        let replacement = match it.arguments.first() {
                            Some(delta) => {
                                let delta_text = self.slice_with_props(delta.span());
                                format!("wx.navigateBack({{ delta: {} }})", delta_text)
                            }
                            None => "wx.navigateBack()".to_string(),
                        };
                        self.edits.push(Edit { start: it.span.start, end: it.span.end, text: replacement });
                        true
                    }
                    "switchTab" => {
                        let Some(route_arg) = it.arguments.first() else {
                            let (line, col) = line_col(self.src, it.span.start as usize, self.line_offset);
                            self.errors.push(format!(
                                "M1021 at line {}:{}: navigate.switchTab(route) requires a literal route string",
                                line, col
                            ));
                            return true;
                        };
                        let Some(route) = self.route_literal(route_arg, it.span) else { return true };
                        let (line, col) = line_col(self.src, it.span.start as usize, self.line_offset);
                        self.route_refs.push(RouteRef {
                            route: route.clone(),
                            kind: RouteRefKind::SwitchTab,
                            line,
                            col,
                        });
                        let replacement = format!("wx.switchTab({{ url: '{}' }})", route);
                        self.edits.push(Edit { start: it.span.start, end: it.span.end, text: replacement });
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }
}

/// `.name` / `[i].x` → store-relative path expr: `` `name` `` / `` `[${i}].x` ``,
/// or `null` for whole-value replacement.
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

/// `todos.value[i].done` → ("todos", "[i].done") → path expr `` `todos[${i}].done` ``
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
            let rhs = self.slice_with_props(it.right.span());
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
            let rhs = self.slice_with_props(it.right.span());
            let replacement = match it.operator {
                AssignmentOperator::Assign => format!("{}.__set({}, {})", acc, path, rhs),
                op => {
                    let base = op.as_str().trim_end_matches('=');
                    format!("{}.__set({}, {}.value{} {} ({}))", acc, path, acc, rest, base, rhs)
                }
            };
            self.edits.push(Edit { start: it.span.start, end: it.span.end, text: replacement });
        } else if let Some((root, source, suffix)) = self.alias_target(left_text, it.span.start) {
            if !suffix.is_empty() {
                let site = text(self.src, it.span).to_string();
                let fix = format!("{}{} = ...", source, suffix);
                self.push_m1001(it.span, &site, &root, &source, &fix);
            }
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
        } else if let Some((root, source, suffix)) = self.alias_target(target_text, it.span.start) {
            if !suffix.is_empty() {
                let site = text(self.src, it.span).to_string();
                let op = if it.operator == UpdateOperator::Increment { "++" } else { "--" };
                let fix = format!("{}{}{}", source, suffix, op);
                self.push_m1001(it.span, &site, &root, &source, &fix);
            }
        }
        walk::walk_update_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if self.try_navigate_call(it) {
            walk::walk_call_expression(self, it);
            return;
        }
        // whitelist mutating array methods on state/store paths: push; reject the rest
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
                // `.slice().reverse()` operates on a copy — only bare paths are writes
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
                                .map(|a| self.slice_with_props(a.span()))
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
                                .map(|a| self.slice_with_props(a.span()))
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
            } else if let Some((root, source, suffix)) = self.alias_target(obj_text, it.span.start) {
                if !suffix.contains('(') {
                    if let Some(method) = member.static_property_name() {
                        if ["push", "pop", "splice", "shift", "unshift", "sort", "reverse"]
                            .contains(&method)
                        {
                            let site = format!("{}.{}()", obj_text, method);
                            let fix = format!("{}{}.{}(...)", source, suffix, method);
                            self.push_m1001(it.span, &site, &root, &source, &fix);
                        }
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
