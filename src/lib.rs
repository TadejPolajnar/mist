pub mod frontmatter;
pub mod npm_bundle;
pub mod scope;
pub mod tag_meta;
pub mod sfc;
pub mod tailwind;
pub mod tailwind_cli;
pub mod template;
pub mod wxml;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const RUNTIME: &str = include_str!("../runtime/mist-rt.js");

/// `runtime/mist-rt.js` with comments/blank lines/indent stripped (~25% smaller).
pub fn runtime_js() -> &'static str {
    static STRIPPED: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    STRIPPED.get_or_init(|| {
        let mut out = String::with_capacity(RUNTIME.len());
        for line in RUNTIME.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with("//") || t.starts_with("/*") || t.starts_with('*') {
                continue;
            }
            out.push_str(t);
            out.push('\n');
        }
        out
    })
}

#[derive(Debug)]
pub struct Output {
    pub wxml: String,
    pub js: String,
    pub wxss: String,
    pub json: Option<String>,
}

pub struct Unit {
    pub output: Output,
    /// (local name, import path) for every used `.mist` component import
    pub used_imports: Vec<(String, String)>,
    /// used imports that were inlined as template partials
    pub used_inline_locals: Vec<String>,
    /// raw class tokens used in the template
    pub classes: Vec<String>,
    /// raw `<style>` content
    pub style: String,
    /// `<style global>` — hoisted to app.wxss in directory builds
    pub style_global: bool,
    /// relative paths of imported store modules
    pub store_import_paths: Vec<String>,
    /// bare npm packages imported by this unit (bundled in project builds)
    pub npm_packages: Vec<String>,
    pub warnings: Vec<String>,
    /// `navigate(...)` call sites with literal routes — validated against the
    /// project route set once it exists (M1021); always empty for flat/single-unit
    /// builds, which have no route set to check against.
    pub route_refs: Vec<frontmatter::RouteRef>,
}

/// Output layout: `Flat` puts everything in one directory (single-entry builds);
/// `Nested` uses the WeChat convention `pages/<name>/<name>.*` / `components/<k>/<k>.*`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Layout {
    Flat,
    Nested,
}

/// Default nesting depth for `pages/<name>/<name>` and `components/<k>/<k>` —
/// two path segments below the dist root.
const DEFAULT_DEPTH: usize = 2;

impl Layout {
    /// `../` repeated `depth` times — the climb from a unit's own output
    /// directory back up to the dist root.
    fn up(depth: usize) -> String {
        "../".repeat(depth)
    }
    fn rt_require(&self, depth: usize) -> String {
        match self {
            Layout::Flat => "./mist-rt.js".to_string(),
            Layout::Nested => format!("{}mist-rt.js", Self::up(depth)),
        }
    }
    fn tw_import(&self, depth: usize) -> String {
        match self {
            Layout::Flat => "./tw-shared.wxss".to_string(),
            Layout::Nested => format!("{}tw-shared.wxss", Self::up(depth)),
        }
    }
    fn tw_theme_import(&self, depth: usize) -> String {
        match self {
            Layout::Flat => "./tw-theme.wxss".to_string(),
            Layout::Nested => format!("{}tw-theme.wxss", Self::up(depth)),
        }
    }
    fn component_ref(&self, kebab: &str, depth: usize) -> String {
        match self {
            Layout::Flat => format!("./{}", kebab),
            Layout::Nested => format!("{}components/{}/{}", Self::up(depth), kebab, kebab),
        }
    }
    fn template_ref(&self, kebab: &str, depth: usize) -> String {
        match self {
            Layout::Flat => format!("./{}.wxml", kebab),
            Layout::Nested => format!("{}components/{}/{}.wxml", Self::up(depth), kebab, kebab),
        }
    }
    fn out_path(&self, name: &str, is_page: bool) -> String {
        match self {
            Layout::Flat => name.to_string(),
            Layout::Nested if is_page => format!("pages/{}/{}", name, name),
            Layout::Nested => format!("components/{}/{}", name, name),
        }
    }
    /// output path (no extension) of a subpackage page — `packages/<pkg>/pages/<name>/<name>`
    fn subpkg_out_path(pkg: &str, name: &str) -> String {
        format!("packages/{}/pages/{}/{}", pkg, name, name)
    }
    /// require path from a page/component to a bundled npm vendor file
    fn vendor_require(&self, stem: &str, depth: usize) -> String {
        match self {
            Layout::Flat => format!("./{}.js", stem),
            Layout::Nested => format!("{}vendor/{}.js", Self::up(depth), stem),
        }
    }
    /// require path from a page/component to a compiled store module
    fn store_require(&self, stem: &str, depth: usize) -> String {
        match self {
            Layout::Flat => format!("./{}.js", stem),
            Layout::Nested => format!("{}stores/{}.js", Self::up(depth), stem),
        }
    }
    /// output path (no extension) of a compiled store module
    fn store_out_path(&self, stem: &str) -> String {
        match self {
            Layout::Flat => stem.to_string(),
            Layout::Nested => format!("stores/{}", stem),
        }
    }
    /// path prefix from a compiled store module to the vendor dir
    fn store_vendor_prefix(&self) -> &'static str {
        match self {
            Layout::Flat => "./",
            Layout::Nested => "../vendor/",
        }
    }
    /// require path from a compiled store module to the runtime
    fn store_rt_require(&self) -> &'static str {
        match self {
            Layout::Flat => "./mist-rt.js",
            Layout::Nested => "../mist-rt.js",
        }
    }
}

pub fn compile(source: &str) -> Result<Output, String> {
    Ok(compile_unit(source, true)?.output)
}

pub fn compile_unit(source: &str, is_page: bool) -> Result<Unit, String> {
    compile_unit_full(source, is_page, &[], Layout::Flat, DEFAULT_DEPTH, "unit", &|_| None)
}

fn template_const_refs(wxml: &str, names: &[String]) -> Vec<String> {
    names
        .iter()
        .filter(|name| {
            let mut rest = wxml;
            while let Some(open) = rest.find("{{") {
                let after = &rest[open + 2..];
                let Some(close) = after.find("}}") else {
                    return false;
                };
                if wxml::word_in(&after[..close], name) {
                    return true;
                }
                rest = &after[close + 2..];
            }
            false
        })
        .cloned()
        .collect()
}

pub fn compile_unit_with_stores(
    source: &str,
    is_page: bool,
    resolve_store: &dyn Fn(&str) -> Option<frontmatter::StoreModuleInfo>,
) -> Result<Unit, String> {
    compile_unit_full(source, is_page, &[], Layout::Flat, DEFAULT_DEPTH, "unit", resolve_store)
}

/// `inline`: local names of imported components to inline as `<template>` uses.
/// `depth`: nesting depth of this unit's own output directory below dist root
/// (ignored for `Layout::Flat`) — `2` for `pages/<n>/<n>` and `components/<k>/<k>`,
/// `4` for subpackage pages at `packages/<pkg>/pages/<n>/<n>`.
fn compile_unit_full(
    source: &str,
    is_page: bool,
    inline: &[String],
    layout: Layout,
    depth: usize,
    scope_name: &str,
    resolve_store: &dyn Fn(&str) -> Option<frontmatter::StoreModuleInfo>,
) -> Result<Unit, String> {
    compile_unit_full_route(source, is_page, inline, layout, depth, scope_name, None, None, resolve_store)
}

#[allow(clippy::too_many_arguments)]
fn compile_unit_full_route(
    source: &str,
    is_page: bool,
    inline: &[String],
    layout: Layout,
    depth: usize,
    scope_name: &str,
    route_param: Option<&str>,
    project_min_lib: Option<&str>,
    resolve_store: &dyn Fn(&str) -> Option<frontmatter::StoreModuleInfo>,
) -> Result<Unit, String> {
    let sfc = sfc::split(source)?;
    let mut analysis = frontmatter::analyze_with_stores_bound(sfc.frontmatter, resolve_store, sfc.frontmatter_line, Some(sfc.template))?;
    for si in &mut analysis.store_imports {
        let stem = Path::new(&si.path).file_stem().unwrap_or_default().to_string_lossy().to_string();
        si.require_path = layout.store_require(&stem, depth);
    }
    for ni in &mut analysis.npm_imports {
        ni.require_path = layout.vendor_require(&npm_bundle::vendor_stem(&ni.package), depth);
    }

    let mut reactive: Vec<String> = analysis.states.iter().map(|s| s.name.clone()).collect();
    reactive.extend(analysis.deriveds.iter().map(|d| d.name.clone()));
    // store mirrors bind in templates just like local state
    reactive.extend(analysis.store_imports.iter().flat_map(|si| si.stores.iter().cloned()));

    let component_locals: Vec<String> = analysis
        .imports
        .iter()
        .map(|i| i.local.clone())
        .filter(|l| !inline.contains(l))
        .collect();

    const PAGE_ONLY_HOOKS: &[&str] = &[
        "onPullDownRefresh",
        "onReachBottom",
        "onPageScroll",
        "onTabItemTap",
        "onShareAppMessage",
        "onShareTimeline",
        "onAddToFavorites",
        "onRouteDone",
        "onSaveExitState",
    ];
    const APP_ONLY_HOOKS: &[&str] =
        &["onError", "onPageNotFound", "onUnhandledRejection", "onThemeChange"];
    const COMPONENT_ONLY_HOOKS: &[&str] = &["onCreate", "onMove", "onAttach", "onDetach"];
    for l in &analysis.lifecycles {
        if is_page && (l.hook == "onPageShow" || l.hook == "onPageHide") {
            return Err(format!(
                "M1013 at line {}:{}: {} is component-only (it maps to pageLifetimes)\n  help: pages use {}",
                l.line,
                l.col,
                l.hook,
                if l.hook == "onPageShow" { "onShow" } else { "onHide" }
            ));
        }
        if !is_page && PAGE_ONLY_HOOKS.contains(&l.hook.as_str()) {
            return Err(format!(
                "M1013 at line {}:{}: {} is page-only — WeChat components never receive it\n  help: declare it in the page that hosts this component",
                l.line, l.col, l.hook
            ));
        }
        if !is_page {
            let suggestion = match l.hook.as_str() {
                "onShow" => Some("onShow is a page lifecycle — components use onPageShow (pageLifetimes.show)"),
                "onHide" => Some("onHide is a page lifecycle — components use onPageHide (pageLifetimes.hide)"),
                "onLoad" => Some("onLoad is a page lifecycle — components use onAttach"),
                "onUnload" => Some("onUnload is a page lifecycle — components use onDetach"),
                _ => None,
            };
            if let Some(help) = suggestion {
                return Err(format!(
                    "M1013 at line {}:{}: {} is page-only — WeChat components never receive it\n  help: {}",
                    l.line, l.col, l.hook, help
                ));
            }
        }
        if is_page && COMPONENT_ONLY_HOOKS.contains(&l.hook.as_str()) {
            let help = match l.hook.as_str() {
                "onAttach" => "pages use onLoad",
                "onDetach" => "pages use onUnload",
                _ => "declare it in the component",
            };
            return Err(format!(
                "M1013 at line {}:{}: {} is component-only — WeChat pages never receive it\n  help: {}",
                l.line, l.col, l.hook, help
            ));
        }
        if APP_ONLY_HOOKS.contains(&l.hook.as_str()) {
            return Err(format!(
                "M1013 at line {}:{}: {} is app-only — WeChat never calls it outside App()\n  help: declare it in app.mist",
                l.line, l.col, l.hook
            ));
        }
    }
    if is_page {
        if analysis.virtual_host.is_some() {
            return Err("'virtualHost' is component-only config".to_string());
        }
        if analysis.pure_data_pattern.is_some() {
            return Err("'pureDataPattern' is component-only config".to_string());
        }
        if !analysis.behaviors.is_empty() {
            return Err("'behaviors' is component-only config".to_string());
        }
        if !analysis.external_classes.is_empty() {
            return Err("'externalClasses' is component-only config".to_string());
        }
    }
    let nodes = template::parse_at(sfc.template, sfc.template_line)?;
    let bare = template::bare_reactive_refs(&nodes, &reactive);
    if !bare.is_empty() {
        let msgs: Vec<String> = bare
            .iter()
            .map(|(name, expr)| {
                format!(
                    "M1007: `{}` is reactive — write `{}.value` in the template (in `{{{}}}`)",
                    name, name, expr
                )
            })
            .collect();
        return Err(msgs.join("; "));
    }
    let mut wxml_out = wxml::emit(&nodes, &reactive, &component_locals, inline)?;

    // wx:key per derived, resolved from template loops rendering that derived
    let loops = template::for_lists(&nodes);
    let mut warnings = Vec::new();
    for (list, key) in &loops {
        if key.is_none()
            && reactive.iter().any(|r| *list == format!("{}.value", r) || list == r)
        {
            warnings.push(format!(
                "M1008: list rendered from `{}` has no key= — updates resend the whole array instead of per-item paths\n  help: add key={{item.<id>}} to the .map() element",
                list
            ));
        }
    }
    for (parent_tag, child_tag) in template::text_box_child_violations(&nodes) {
        let child_emitted = wxml::map_tag(&child_tag).unwrap_or(child_tag.as_str());
        let child_desc = if child_emitted == child_tag {
            format!("<{}>", child_tag)
        } else {
            format!("<{}> (→ <{}>)", child_tag, child_emitted)
        };
        warnings.push(format!(
            "M1018: <{}> maps to native <text>, which renders inline-only — child {} will ignore box styling; use <div> for the parent or restructure",
            parent_tag, child_desc
        ));
    }
    if let Some(config) = analysis.config.as_deref() {
        if let Ok(json) = frontmatter::config_literal_to_json(config) {
            let compact = json.replace(' ', "");
            let has_hook = |h: &str| analysis.lifecycles.iter().any(|l| l.hook == h);
            if compact.contains("\"enablePullDownRefresh\":true") && !has_hook("onPullDownRefresh")
            {
                warnings.push(
                    "M1012: config enables pull-down refresh but no onPullDownRefresh hook is declared — the spinner will never stop\n  help: import { onPullDownRefresh } from 'mist' and call wx.stopPullDownRefresh() when done".to_string(),
                );
            }
            if compact.contains("\"onReachBottomDistance\":") && !has_hook("onReachBottom") {
                warnings.push(
                    "M1012: config sets onReachBottomDistance but no onReachBottom hook is declared\n  help: import { onReachBottom } from 'mist'".to_string(),
                );
            }
        }
    }
    let derived_keys: Vec<Option<String>> = analysis
        .deriveds
        .iter()
        .map(|d| {
            let value_form = format!("{}.value", d.name);
            loops
                .iter()
                .find(|(list, _)| *list == value_form || *list == d.name)
                .and_then(|(_, key)| key.clone())
        })
        .collect();

    let scoping = if sfc.style_scoped {
        Some(scope::scope_style(sfc.style.unwrap_or(""), scope_name))
    } else {
        None
    };
    if let Some((_, names)) = &scoping {
        for &i in &wxml_out.class_hoists {
            wxml_out.hoisted[i] = scope::scope_class_expr(&wxml_out.hoisted[i], names, scope_name);
        }
    }

    // template-hoisted expressions become generated deriveds
    let hoists = frontmatter::hoisted_deriveds(&analysis, &wxml_out.hoisted, &wxml_out.for_hoists)?;
    let mut derived_keys = derived_keys;
    for (d, key) in hoists {
        analysis.deriveds.push(d);
        derived_keys.push(key);
    }

    if !analysis.trusted_packages.is_empty() {
        return Err(
            "config.trustedPackages is app-level — declare it in app.mist so the whole project's bundles share one allowlist".to_string(),
        );
    }
    if analysis.size_budget.is_some() {
        return Err(
            "config.sizeBudget is app-level — declare it in app.mist; packages are measured whole".to_string(),
        );
    }
    let route_seed = match route_param {
        Some(param) => match analysis.states.iter().find(|s| s.name == param) {
            Some(state) => Some((param, state.bound)),
            None => {
                return Err(format!(
                    "M1025: [{param}].mist declares route param '{param}' but the frontmatter has no `const {param} = state(...)` — declare it; the compiler seeds it from the query",
                    param = param
                ));
            }
        },
        None => None,
    };
    let multiple_slots = !is_page && template::has_named_slot(&nodes);
    let const_seeds = template_const_refs(&wxml_out.wxml, &analysis.plain_consts);
    let js = frontmatter::emit_js_route(
        &analysis,
        &wxml_out.handlers,
        &derived_keys,
        is_page,
        multiple_slots,
        &layout.rt_require(depth),
        &wxml_out.vbinds,
        &const_seeds,
        route_seed,
    );

    let import_path_of = |local: &String| -> Option<String> {
        analysis.imports.iter().find(|i| &i.local == local).map(|i| i.path.clone())
    };

    let used_imports: Vec<(String, String)> = wxml_out
        .used_components
        .iter()
        .chain(wxml_out.used_inline.iter())
        .filter_map(|l| import_path_of(l).map(|p| (l.clone(), p)))
        .collect();

    let mut using: BTreeMap<String, String> = wxml_out
        .used_components
        .iter()
        .filter_map(|l| import_path_of(l).map(|p| (l, p)))
        .map(|(local, path)| {
            let stem = Path::new(&path).file_stem().unwrap_or_default().to_string_lossy().to_string();
            (wxml::kebab(local), layout.component_ref(&wxml::kebab(&stem), depth))
        })
        .collect();

    for (name, target) in &analysis.plugin_components {
        if using.contains_key(name) {
            return Err(format!(
                "M1015: config.pluginComponents['{}'] collides with an imported .mist component of the same tag",
                name
            ));
        }
        using.insert(name.clone(), target.clone());
    }

    if !wxml_out.unknown_tags.is_empty() {
        let manual_using = analysis
            .config
            .as_deref()
            .map(frontmatter::config_using_components_keys)
            .transpose()?
            .unwrap_or_default();
        for (tag, suggestion) in &wxml_out.unknown_tags {
            if using.contains_key(tag)
                || manual_using.iter().any(|k| k == tag)
                || analysis.custom_tags.iter().any(|t| t == tag)
            {
                continue;
            }
            let hint = suggestion
                .as_ref()
                .map(|s| format!("; did you mean <{}>?", s))
                .unwrap_or_default();
            warnings.push(format!(
                "M1019: unknown tag <{}> — WeChat renders unknown tags as nothing{}",
                tag, hint
            ));
        }
    }

    let effective_min_lib = analysis.min_lib_version.as_deref().or(project_min_lib);
    warnings.extend(meta_warning_texts(&wxml_out, &analysis.custom_attrs, effective_min_lib));

    let json = build_json(
        analysis.config.as_deref(),
        &using,
        is_page,
        sfc.style_global && matches!(layout, Layout::Nested),
    )?;

    // inlined children render inside this unit — import their template partials
    let mut wxml = String::new();
    for local in &wxml_out.used_inline {
        if let Some(path) = import_path_of(local) {
            let stem = Path::new(&path).file_stem().unwrap_or_default().to_string_lossy().to_string();
            wxml.push_str(&format!("<import src=\"{}\" />\n", layout.template_ref(&wxml::kebab(&stem), depth)));
        }
    }
    wxml.push_str(&wxml_out.wxml);

    let mut classes = tailwind::extract_classes(&nodes);
    let mut style = sfc.style.unwrap_or("").to_string();
    if let Some((scoped_css, names)) = scoping {
        style = scoped_css;
        wxml = scope::scope_wxml(&wxml, &names, scope_name);
        for c in classes.iter_mut() {
            if names.contains(c.as_str()) {
                *c = scope::suffixed(c, scope_name);
            }
        }
    }
    let wxss = assemble_wxss(&classes, &style, &[], layout, depth, is_page);

    let store_import_paths: Vec<String> =
        analysis.store_imports.iter().map(|si| si.path.clone()).collect();
    let npm_packages: Vec<String> =
        analysis.npm_imports.iter().map(|ni| ni.package.clone()).collect();

    Ok(Unit {
        output: Output { wxml, js, wxss, json },
        used_imports,
        used_inline_locals: wxml_out.used_inline,
        classes,
        style,
        style_global: sfc.style_global,
        store_import_paths,
        npm_packages,
        warnings,
        route_refs: analysis.route_refs,
    })
}

/// Compile a pure-render component as a `<template name="...">` partial:
/// no JS, no JSON — it renders in its parent's context.
fn meta_warning_texts(
    wxml_out: &wxml::WxmlOutput,
    custom_attrs: &[String],
    min_lib: Option<&str>,
) -> Vec<String> {
    let mut warnings = Vec::new();
    for w in &wxml_out.meta_warnings {
        if custom_attrs.iter().any(|a| a == &w.name) {
            continue;
        }
        let hint = w
            .suggestion
            .as_ref()
            .map(|s| format!("; did you mean {}?", s))
            .unwrap_or_default();
        if w.is_event {
            warnings.push(format!(
                "M1023: unknown event {} on <{}> — WeChat silently ignores unknown events{}\n  help: add '{}' to config.customAttrs to suppress",
                w.name, w.tag, hint, w.name
            ));
        } else {
            warnings.push(format!(
                "M1024: unknown attribute '{}' on <{}> — WeChat silently ignores unknown attributes{}\n  help: add '{}' to config.customAttrs to suppress",
                w.name, w.tag, hint, w.name
            ));
        }
    }
    if let Some(min) = min_lib {
        for (tag, name, since) in &wxml_out.since_hits {
            if tag_meta::version_lt(min, since) {
                warnings.push(format!(
                    "M1027: {} on <{}> needs base library ≥ {} but config.minLibVersion is '{}'\n  help: raise minLibVersion or drop the feature; the app's admin-console minimum must match",
                    name, tag, since, min
                ));
            }
        }
    }
    warnings
}

fn compile_template_unit(source: &str, name: &str, project_min_lib: Option<&str>) -> Result<Unit, String> {
    let sfc = sfc::split(source)?;
    let nodes = template::parse_at(sfc.template, sfc.template_line)?;
    let wxml_out = wxml::emit(&nodes, &[], &[], &[])?;
    let template_warnings = meta_warning_texts(&wxml_out, &[], project_min_lib);
    let body: String = wxml_out.wxml.lines().map(|l| format!("  {}\n", l)).collect();
    let mut wxml = format!("<template name=\"{}\">\n{}</template>\n", name, body);
    let mut classes = tailwind::extract_classes(&nodes);
    let mut style = sfc.style.unwrap_or("").to_string();
    if sfc.style_scoped {
        let (scoped_css, names) = scope::scope_style(&style, name);
        style = scoped_css;
        wxml = scope::scope_wxml(&wxml, &names, name);
        for c in classes.iter_mut() {
            if names.contains(c.as_str()) {
                *c = scope::suffixed(c, name);
            }
        }
    }
    Ok(Unit {
        output: Output { wxml, js: String::new(), wxss: String::new(), json: None },
        used_imports: Vec::new(),
        used_inline_locals: Vec::new(),
        classes,
        style,
        style_global: sfc.style_global,
        store_import_paths: Vec::new(),
        npm_packages: Vec::new(),
        warnings: template_warnings,
        route_refs: Vec::new(),
    })
}

/// Can this component melt into its parents as a `<template>` partial?
/// Pure render only: props in, markup out.
fn is_inlinable(source: &str) -> bool {
    let Ok(sfc) = sfc::split(source) else { return false };
    let Ok(analysis) = frontmatter::analyze(sfc.frontmatter) else { return false };
    let Ok(nodes) = template::parse(sfc.template) else { return false };
    analysis.states.is_empty()
        && analysis.deriveds.is_empty()
        && analysis.methods.is_empty()
        && analysis.lifecycles.is_empty()
        && analysis.callback_props.is_empty()
        && analysis.imports.is_empty()
        && analysis.plain_stmts.is_empty()
        && analysis.config.is_none()
        && analysis.inline != Some(false)
        && !template::has_slot(&nodes)
        && !template::has_events(&nodes)
}

fn assemble_wxss(
    classes: &[String],
    own_style: &str,
    merged_styles: &[String],
    layout: Layout,
    depth: usize,
    is_page: bool,
) -> String {
    let mut wxss = String::new();
    // engine-agnostic: any class usage imports the shared sheet; the `page {}`
    // theme sheet is legal only in page WXSS, so components never import it
    if !classes.is_empty() {
        if is_page {
            wxss.push_str(&format!("@import \"{}\";\n", layout.tw_theme_import(depth)));
        }
        wxss.push_str(&format!("@import \"{}\";\n", layout.tw_import(depth)));
    }
    if !own_style.is_empty() {
        wxss.push_str(own_style);
        wxss.push('\n');
    }
    for s in merged_styles {
        if !s.is_empty() {
            wxss.push_str(s);
            wxss.push('\n');
        }
    }
    wxss
}

#[derive(Debug)]
pub struct CompiledFile {
    /// output basename, e.g. `todo` or `todo-item`
    pub name: String,
    /// output path relative to dist, without extension — e.g. `pages/index/index`
    pub out_path: String,
    pub is_page: bool,
    /// subpackage root name (e.g. `shop`) when this is a subpackage page; `None` for
    /// main-package pages, components and stores
    pub package: Option<String>,
    pub output: Output,
    /// this unit's `navigate(...)` literal-route call sites, for late M1021 validation
    pub route_refs: Vec<frontmatter::RouteRef>,
    /// `[param].mist` route pages: the declared query param (drives typed routes)
    pub route_param: Option<String>,
}

#[derive(Debug)]
pub struct Project {
    pub files: Vec<CompiledFile>,
    /// generated shared Tailwind utilities (empty when no known classes are used)
    pub tailwind_css: String,
    /// `page { ... }` theme variables — written to `tw-theme.wxss`, imported by pages only
    pub tailwind_theme_css: String,
    pub unknown_classes: Vec<String>,
    /// rules removed because WXSS cannot express their selector
    pub dropped_selectors: Vec<String>,
    /// per-unit template warnings (e.g. M1008), prefixed with the source path
    pub warnings: Vec<String>,
    /// class → first source file that used it (for M1002 attribution)
    pub class_sources: std::collections::BTreeMap<String, String>,
    /// app.mist's `config.sizeBudget` in bytes — opt-in M1029 threshold
    pub size_budget: Option<u64>,
    /// present for directory builds: app.js/app.json/app.wxss
    pub app: Option<AppShell>,
}

/// Compile an entry page and, recursively, every `.mist` component it imports.
/// Pure-render components are inlined into their parents as `<template>` partials.
/// Utilities are generated by the Tailwind v4 CLI and post-processed for WXSS.
pub fn compile_project(entry: &Path) -> Result<Project, String> {
    let mut ctx = new_project_ctx(Layout::Flat);
    if let Some(parent) = entry.parent() {
        ctx.theme = std::fs::read_to_string(parent.join("theme.css")).ok();
    }
    compile_rec(entry, UnitKind::Page, DEFAULT_DEPTH, None, &mut ctx)?;
    finish_project(ctx, None)
}

/// Compile a `src/` directory: `app.mist` + every page in `pages/*.mist`.
/// Output uses the WeChat layout: `pages/<name>/<name>.*`, `components/<k>/<k>.*`,
/// `app.js`/`app.json`/`app.wxss` at the root.
pub fn compile_project_dir(src: &Path) -> Result<Project, String> {
    let app_path = src.join("app.mist");
    let app_source = std::fs::read_to_string(&app_path)
        .map_err(|e| format!("cannot read {}: {}", app_path.display(), e))?;

    let pages_dir = src.join("pages");
    let pages_entries: Vec<PathBuf> = std::fs::read_dir(&pages_dir)
        .map_err(|e| format!("cannot read {}: {}", pages_dir.display(), e))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    let mut page_paths: Vec<PathBuf> = pages_entries
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "mist"))
        .cloned()
        .collect();
    page_paths.sort();
    // index first — it becomes the launch page
    page_paths.sort_by_key(|p| p.file_stem().map(|s| s != "index").unwrap_or(true));
    if page_paths.is_empty() {
        return Err(format!(
            "no pages found in {} — the main package needs at least one page in src/pages/",
            pages_dir.display()
        ));
    }
    if let Some(stray) = page_paths.iter().find(|p| route_param_of(p).is_some()) {
        let param = route_param_of(stray).unwrap_or_default();
        return Err(format!(
            "pages/[{param}].mist must live in a page directory — move it to pages/<name>/[{param}].mist",
            param = param
        ));
    }

    let mut ctx = new_project_ctx(Layout::Nested);
    let app_pre = sfc::split(&app_source).ok().and_then(|s| frontmatter::analyze(s.frontmatter).ok());
    ctx.min_lib = app_pre.as_ref().and_then(|a| a.min_lib_version.clone());
    ctx.size_budget = app_pre.as_ref().and_then(|a| a.size_budget);
    let trusted_packages: Vec<String> =
        app_pre.map(|a| a.trusted_packages).unwrap_or_default();
    ctx.theme = std::fs::read_to_string(src.join("theme.css")).ok();
    warn_dropped_mist_subdirs(&pages_entries, "pages", &mut ctx);
    let mut unit_errors: Vec<String> = Vec::new();
    for page in &page_paths {
        if let Err(e) = compile_rec(page, UnitKind::Page, DEFAULT_DEPTH, None, &mut ctx) {
            unit_errors.push(e);
        }
    }
    for (path, page_name, param) in discover_route_param_pages(&pages_entries, "pages")? {
        if page_paths.iter().any(|p| p.file_stem().is_some_and(|s| s == page_name.as_str())) {
            return Err(format!(
                "pages/{name}.mist and pages/{name}/[{param}].mist both compile to pages/{name}/{name} — keep one",
                name = page_name,
                param = param
            ));
        }
        if let Err(e) = compile_rec_at(
            &path,
            UnitKind::Page,
            DEFAULT_DEPTH,
            None,
            None,
            Some((&page_name, &param)),
            &mut ctx,
        ) {
            unit_errors.push(e);
        }
    }

    let packages_dir = src.join("packages");
    if let Ok(entries) = std::fs::read_dir(&packages_dir) {
        let mut pkg_dirs: Vec<PathBuf> =
            entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_dir()).collect();
        pkg_dirs.sort();
        for pkg_dir in &pkg_dirs {
            let pkg = pkg_dir.file_name().unwrap_or_default().to_string_lossy().to_string();
            validate_package_name(&pkg)
                .map_err(|e| format!("{}: {}", pkg_dir.display(), e))?;
            let pkg_pages_dir = pkg_dir.join("pages");
            let pkg_pages_entries: Vec<PathBuf> = std::fs::read_dir(&pkg_pages_dir)
                .map_err(|e| format!("cannot read {}: {}", pkg_pages_dir.display(), e))?
                .filter_map(|entry| entry.ok().map(|e| e.path()))
                .collect();
            let mut pkg_page_paths: Vec<PathBuf> = pkg_pages_entries
                .iter()
                .filter(|p| p.extension().is_some_and(|e| e == "mist"))
                .cloned()
                .collect();
            pkg_page_paths.sort();
            warn_dropped_mist_subdirs(
                &pkg_pages_entries,
                &format!("packages/{}/pages", pkg),
                &mut ctx,
            );
            for page in &pkg_page_paths {
                if let Err(e) = compile_rec(page, UnitKind::Page, SUBPKG_DEPTH, Some(&pkg), &mut ctx) {
                    unit_errors.push(e);
                }
            }
            for (path, page_name, param) in
                discover_route_param_pages(&pkg_pages_entries, &format!("packages/{}/pages", pkg))?
            {
                if pkg_page_paths
                    .iter()
                    .any(|p| p.file_stem().is_some_and(|s| s == page_name.as_str()))
                {
                    return Err(format!(
                        "packages/{pkg}/pages/{name}.mist and packages/{pkg}/pages/{name}/[{param}].mist both compile to the same page — keep one",
                        pkg = pkg,
                        name = page_name,
                        param = param
                    ));
                }
                if let Err(e) = compile_rec_at(
                    &path,
                    UnitKind::Page,
                    SUBPKG_DEPTH,
                    Some(&pkg),
                    None,
                    Some((&page_name, &param)),
                    &mut ctx,
                ) {
                    unit_errors.push(e);
                }
            }
        }
    }

    let custom_tab_bar_path = src.join("custom-tab-bar.mist");
    let custom_tab_bar_exists = custom_tab_bar_path.is_file();
    if custom_tab_bar_exists {
        if let Err(e) = compile_rec_at(
            &custom_tab_bar_path,
            UnitKind::Component,
            CUSTOM_TAB_BAR_DEPTH,
            None,
            Some("custom-tab-bar/index"),
            None,
            &mut ctx,
        ) {
            unit_errors.push(e);
        }
    }

    let mut dedup = std::collections::HashSet::new();
    unit_errors.retain(|e| dedup.insert(e.clone()));
    if !unit_errors.is_empty() {
        return Err(unit_errors.join("\n\n"));
    }

    let npm_usage = std::mem::take(&mut ctx.npm_usage);
    if !npm_usage.is_empty() {
        let parent = src.parent().unwrap_or(src);
        let project_root = if parent.join("node_modules").exists() {
            parent.to_path_buf()
        } else {
            src.to_path_buf()
        };
        for (pkg, users) in &npm_usage {
            let js = npm_bundle::bundle_package(&project_root, pkg)?;
            if !trusted_packages.iter().any(|t| t == pkg) {
                let hits = npm_bundle::foreign_api_hits(&js);
                if !hits.is_empty() {
                    ctx.warnings.push(format!(
                        "M1028: npm package '{}' references {} — these APIs don't exist in WeChat's JS runtime and fail when reached\n  help: if the references are guarded feature detection, add '{}' to app.mist config.trustedPackages",
                        pkg,
                        hits.join(", "),
                        pkg
                    ));
                }
            }
            let stem = npm_bundle::vendor_stem(pkg);
            // a vendor imported by exactly one subpackage moves into it — the
            // main package keeps only vendors that main or several packages use
            let sole_sub = match users.iter().collect::<Vec<_>>().as_slice() {
                [Some(sub)] => Some(sub.clone()),
                _ => None,
            };
            let out_path = match &sole_sub {
                Some(sub) => format!("packages/{}/vendor/{}", sub, stem),
                None => format!("vendor/{}", stem),
            };
            if let Some(sub) = &sole_sub {
                let old = format!("require('../../../../vendor/{}.js')", stem);
                let new = format!("require('../../vendor/{}.js')", stem);
                for f in ctx.files.iter_mut() {
                    if f.package.as_deref() == Some(sub.as_str()) && f.output.js.contains(&old) {
                        f.output.js = f.output.js.replace(&old, &new);
                    }
                }
            }
            ctx.files.push(CompiledFile {
                name: stem,
                out_path,
                is_page: false,
                package: sole_sub,
                output: Output { wxml: String::new(), js, wxss: String::new(), json: None },
                route_refs: Vec::new(),
                route_param: None,
            });
        }
    }

    let app = compile_app(&app_source, &mut ctx, custom_tab_bar_exists)?;
    let tab_bar_paths = app_tab_bar_page_paths(&app_source);
    validate_routes(&ctx, tab_bar_paths.as_deref())?;
    finish_project(ctx, Some(app))
}

/// `custom-tab-bar/index` sits one segment below dist root.
const CUSTOM_TAB_BAR_DEPTH: usize = 1;

/// `pages/<n>/<n>` and `components/<k>/<k>` sit two segments below dist root;
/// `packages/<pkg>/pages/<n>/<n>` sits four.
const SUBPKG_DEPTH: usize = 4;

const RESERVED_PACKAGE_NAMES: &[&str] = &["pages", "components", "stores", "assets"];

fn validate_package_name(name: &str) -> Result<(), String> {
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err(format!(
            "invalid subpackage name '{}' — use alphanumeric characters, '-' or '_'",
            name
        ));
    }
    if RESERVED_PACKAGE_NAMES.contains(&name) {
        return Err(format!(
            "'{}' is a reserved dist path and cannot be used as a subpackage name",
            name
        ));
    }
    Ok(())
}

/// M1016: does this directory contain a `.mist` file anywhere below it?
fn dir_contains_mist(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else { return false };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if dir_contains_mist(&path) {
                return true;
            }
        } else if path.extension().is_some_and(|e| e == "mist") {
            return true;
        }
    }
    false
}

/// M1016: warn for every immediate subdirectory of `pages_entries` that has
/// `.mist` files sitting in it (or nested below it) instead of directly in
/// the pages root — those files are silently dropped from compilation.
/// `label` is the pages-root path relative to `src`, e.g. `pages` or
/// `packages/shop/pages`.
fn route_param_of(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let inner = stem.strip_prefix('[')?.strip_suffix(']')?;
    let mut chars = inner.chars();
    let first = chars.next()?;
    if !(first.is_alphabetic() || first == '_') || !chars.all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    Some(inner.to_string())
}

/// `pages/<dir>/[<param>].mist` route-param pages: returns (page path, page
/// name, param) per qualifying dir. Errors on two bracket files in one dir.
fn discover_route_param_pages(
    pages_entries: &[PathBuf],
    label: &str,
) -> Result<Vec<(PathBuf, String, String)>, String> {
    let mut out = Vec::new();
    for dir_entry in pages_entries.iter().filter(|p| p.is_dir()) {
        let name = dir_entry.file_name().unwrap_or_default().to_string_lossy().to_string();
        let Ok(entries) = std::fs::read_dir(dir_entry) else { continue };
        let mut found: Vec<(PathBuf, String)> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "mist"))
            .filter_map(|p| route_param_of(&p).map(|param| (p, param)))
            .collect();
        found.sort();
        if found.len() > 1 {
            return Err(format!(
                "{}/{}/ has more than one [param].mist file — a page directory takes exactly one",
                label, name
            ));
        }
        if let Some((path, param)) = found.pop() {
            out.push((path, name, param));
        }
    }
    Ok(out)
}

fn warn_dropped_mist_subdirs(pages_entries: &[PathBuf], label: &str, ctx: &mut ProjectCtx) {
    for dir_entry in pages_entries.iter().filter(|p| p.is_dir()) {
        let has_route_page = std::fs::read_dir(dir_entry)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .any(|p| route_param_of(&p).is_some())
            })
            .unwrap_or(false);
        let dropped_others = std::fs::read_dir(dir_entry)
            .map(|entries| {
                entries.filter_map(|e| e.ok().map(|e| e.path())).any(|p| {
                    (p.extension().is_some_and(|e| e == "mist") && route_param_of(&p).is_none())
                        || (p.is_dir() && dir_contains_mist(&p))
                })
            })
            .unwrap_or(false);
        if has_route_page && !dropped_others {
            continue;
        }
        if dir_contains_mist(dir_entry) && (!has_route_page || dropped_others) {
            let name = dir_entry.file_name().unwrap_or_default().to_string_lossy().to_string();
            ctx.warnings.push(format!(
                "M1016: {}/{}/ contains .mist files that are not compiled — pages must sit directly in pages/, in packages/<pkg>/pages/ for subpackages, or be a single [param].mist route page",
                label, name
            ));
        }
    }
}

fn new_project_ctx(layout: Layout) -> ProjectCtx {
    ProjectCtx {
        npm_usage: std::collections::BTreeMap::new(),
        min_lib: None,
        size_budget: None,
        store_info_memo: std::cell::RefCell::new(std::collections::HashMap::new()),
        store_errors: std::cell::RefCell::new(std::collections::HashMap::new()),
        inlinable_memo: std::cell::RefCell::new(std::collections::HashMap::new()),
        seen: Vec::new(),
        files: Vec::new(),
        classes: Vec::new(),
        template_styles: BTreeMap::new(),
        global_styles: Vec::new(),
        store_out_paths: BTreeMap::new(),
        class_sources: BTreeMap::new(),
        style_defined_classes: std::collections::HashSet::new(),
        warnings: Vec::new(),
        layout,
        theme: None,
    }
}

fn compile_app(
    source: &str,
    ctx: &mut ProjectCtx,
    custom_tab_bar_exists: bool,
) -> Result<AppShell, String> {
    let sfc = sfc::split(source).map_err(|e| format!("app.mist: {}", e))?;
    let analysis = frontmatter::analyze(sfc.frontmatter).map_err(|e| format!("app.mist: {}", e))?;
    if let Some(ni) = analysis.npm_imports.first() {
        return Err(format!(
            "app.mist: cannot import '{}' — npm imports work in pages and components, not the app shell",
            ni.package
        ));
    }
    const APP_HOOKS: &[&str] = &[
        "onLaunch", "onShow", "onHide", "onError", "onPageNotFound", "onUnhandledRejection",
        "onThemeChange",
    ];
    for l in &analysis.lifecycles {
        if !APP_HOOKS.contains(&l.hook.as_str()) {
            return Err(format!(
                "app.mist: M1013 at line {}:{}: {} is not an App lifecycle — WeChat never calls it on App()\n  help: app.mist supports onLaunch, onShow, onHide, onError, onPageNotFound, onUnhandledRejection, onThemeChange; declare {} in a page",
                l.line, l.col, l.hook, l.hook
            ));
        }
    }
    if analysis.virtual_host.is_some() {
        return Err("app.mist: 'virtualHost' is component-only config".to_string());
    }
    if analysis.pure_data_pattern.is_some() {
        return Err("app.mist: 'pureDataPattern' is component-only config".to_string());
    }
    if !analysis.behaviors.is_empty() {
        return Err("app.mist: 'behaviors' is component-only config".to_string());
    }
    if !analysis.external_classes.is_empty() {
        return Err("app.mist: 'externalClasses' is component-only config".to_string());
    }
    if !analysis.states.is_empty() || !analysis.deriveds.is_empty() {
        return Err(
            "app.mist cannot declare state()/derived() — App() has no reactive data; use page state (use a store instead)"
                .to_string(),
        );
    }
    if !sfc.template.trim().is_empty() {
        return Err("app.mist cannot have a template — it only defines app lifecycle, config and global styles".to_string());
    }
    let js = frontmatter::emit_app_js(&analysis);

    let main_pages: Vec<String> = ctx
        .files
        .iter()
        .filter(|f| f.is_page && f.package.is_none())
        .map(|f| format!("\"{}\"", f.out_path))
        .collect();
    if main_pages.is_empty() {
        return Err(
            "main package needs at least one page in src/pages/".to_string(),
        );
    }

    let mut sub_pkg_names: Vec<&String> = ctx
        .files
        .iter()
        .filter_map(|f| if f.is_page { f.package.as_ref() } else { None })
        .collect();
    sub_pkg_names.sort();
    sub_pkg_names.dedup();
    let sub_packages: Vec<String> = sub_pkg_names
        .iter()
        .map(|pkg| {
            let root = format!("packages/{}", pkg);
            let prefix = format!("{}/", root);
            let pages: Vec<String> = ctx
                .files
                .iter()
                .filter(|f| f.is_page && f.package.as_deref() == Some(pkg.as_str()))
                .map(|f| {
                    let rel = f.out_path.strip_prefix(&prefix).unwrap_or(&f.out_path);
                    format!("\"{}\"", rel)
                })
                .collect();
            format!(
                "{{ \"root\": \"{}\", \"name\": \"{}\", \"pages\": [{}] }}",
                root,
                pkg,
                pages.join(", ")
            )
        })
        .collect();

    let mut fields = vec![format!("\"pages\": [{}]", main_pages.join(", "))];
    if !sub_packages.is_empty() {
        fields.push(format!("\"subPackages\": [{}]", sub_packages.join(", ")));
    }
    let mut tab_bar_custom = false;
    let mut lazy_declared = false;
    if let Some(config) = analysis.config.as_deref() {
        let keys = frontmatter::config_top_level_keys(config).map_err(|e| format!("app.mist: {}", e))?;
        lazy_declared = keys.iter().any(|k| k == "lazyCodeLoading");
        reject_reserved_key(&keys, "pages", "the page list is generated from src/pages/")
            .map_err(|e| format!("app.mist: {}", e))?;
        reject_reserved_key(&keys, "subPackages", "subpackages are generated from src/packages/")
            .map_err(|e| format!("app.mist: {}", e))?;
        reject_reserved_key(&keys, "sitemapLocation", "place a sitemap.json next to app.mist instead")
            .map_err(|e| format!("app.mist: {}", e))?;
        tab_bar_custom = frontmatter::config_tab_bar_custom(config);
        let json = frontmatter::config_literal_to_json(config).map_err(|e| format!("app.mist: {}", e))?;
        let inner = object_inner(&json).to_string();
        if !inner.is_empty() {
            fields.push(inner);
        }
    }
    if tab_bar_custom && !custom_tab_bar_exists {
        return Err(
            "app.mist: M1020: config.tabBar.custom is true but src/custom-tab-bar.mist does not exist — WeChat would render a blank tab bar\n  help: add src/custom-tab-bar.mist, or remove tabBar.custom".to_string(),
        );
    }
    if custom_tab_bar_exists && !tab_bar_custom {
        ctx.warnings.push(
            "M1020: src/custom-tab-bar.mist exists but app.mist config lacks tabBar.custom: true — WeChat will ignore it and render the built-in tab bar\n  help: set tabBar: { custom: true, ... } in app.mist config".to_string(),
        );
    }
    if !lazy_declared {
        fields.push("\"lazyCodeLoading\": \"requiredComponents\"".to_string());
    }
    fields.push("\"sitemapLocation\": \"sitemap.json\"".to_string());
    let json = format!("{{ {} }}", fields.join(", "));

    if sfc.style_scoped {
        return Err("app.mist <style> cannot be scoped — app styles are global by definition".to_string());
    }
    if sfc.style_global {
        return Err("app.mist <style> is already global — drop the global attribute".to_string());
    }
    let wxss = sfc.style.unwrap_or("").to_string();
    ctx.style_defined_classes.extend(tailwind::harvest_style_classes(&wxss));
    Ok(AppShell { js, json, wxss })
}

#[derive(Debug)]
pub struct AppShell {
    pub js: String,
    pub json: String,
    pub wxss: String,
}

/// `config.tabBar.list[].pagePath` from `app.mist`'s source, as `/`-prefixed
/// routes — `None` when there's no static tab bar list to check
/// `navigate.switchTab` against (falls back to plain route validation).
fn app_tab_bar_page_paths(app_source: &str) -> Option<Vec<String>> {
    let sfc = sfc::split(app_source).ok()?;
    let analysis = frontmatter::analyze(sfc.frontmatter).ok()?;
    let config = analysis.config?;
    frontmatter::config_tab_bar_page_paths(&config)
        .map(|paths| paths.iter().map(|p| format!("/{}", p.trim_start_matches('/'))).collect())
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

/// nearest route within edit distance 3, or `None`
fn suggest_route<'a>(route: &str, candidates: &'a [String]) -> Option<&'a str> {
    candidates
        .iter()
        .map(|c| (c.as_str(), levenshtein(route, c)))
        .filter(|(_, dist)| *dist <= 3)
        .min_by_key(|(_, dist)| *dist)
        .map(|(c, _)| c)
}

/// M1021: validate every collected `navigate(...)` literal route against the
/// project's route set, now that it's fully known (every page/subpackage page
/// has been compiled). `tab_bar_paths`: `Some` when `app.mist`'s
/// `tabBar.list[].pagePath` is statically extractable — `navigate.switchTab`
/// is then checked against that list instead of the full route set.
fn validate_routes(ctx: &ProjectCtx, tab_bar_paths: Option<&[String]>) -> Result<(), String> {
    let all_routes: Vec<String> =
        ctx.files.iter().filter(|f| f.is_page).map(|f| format!("/{}", f.out_path)).collect();
    for file in &ctx.files {
        for r in &file.route_refs {
            let (candidates, is_switch_tab_with_list) = match (r.kind, tab_bar_paths) {
                (frontmatter::RouteRefKind::SwitchTab, Some(tabs)) => (tabs, true),
                _ => (&all_routes[..], false),
            };
            if candidates.iter().any(|c| c == &r.route) {
                continue;
            }
            let suggestion = suggest_route(&r.route, candidates)
                .map(|s| format!("; did you mean '{}'?", s))
                .unwrap_or_default();
            let msg = if is_switch_tab_with_list {
                format!(
                    "M1021 at line {}:{}: '{}' is not a tab-bar page — navigate.switchTab only accepts routes listed in config.tabBar.list{}",
                    r.line, r.col, r.route, suggestion
                )
            } else {
                format!(
                    "M1021 at line {}:{}: unknown route '{}' — not in the compiled page list{}",
                    r.line, r.col, r.route, suggestion
                )
            };
            return Err(format!("{}: {}", file.out_path, msg));
        }
    }
    Ok(())
}

fn finish_project(mut ctx: ProjectCtx, mut app: Option<AppShell>) -> Result<Project, String> {
    if let Some(app) = app.as_mut() {
        for s in &ctx.global_styles {
            if !app.wxss.is_empty() && !app.wxss.ends_with('\n') {
                app.wxss.push('\n');
            }
            app.wxss.push_str(s);
            app.wxss.push('\n');
        }
    }
    ctx.classes.sort();
    ctx.classes.dedup();
    let (tailwind_css, tailwind_theme_css, unknown_classes, dropped_selectors) =
        if ctx.classes.is_empty() {
            (String::new(), String::new(), Vec::new(), Vec::new())
        } else {
            match tailwind_cli::generate(&ctx.classes, ctx.theme.as_deref()) {
                Ok(result) => {
                    // unknown = tailwind produced nothing for it AND it wasn't a rule we dropped
                    let unknown: Vec<String> = ctx
                        .classes
                        .iter()
                        .filter(|c| {
                            !result.css.contains(&format!(".{}", tailwind::sanitize(c)))
                                && !result.dropped_selectors.iter().any(|d| d.contains(c.as_str()))
                                && !ctx.style_defined_classes.contains(c.as_str())
                        })
                        .cloned()
                        .collect();
                    (result.css, result.theme_css, unknown, result.dropped_selectors)
                }
                Err(e) => {
                    let brief: String =
                        e.lines().take(2).collect::<Vec<_>>().join(" ").chars().take(200).collect();
                    ctx.warnings.push(format!(
                        "tailwind generation failed — building without generated CSS (offline? set npm_config_registry to a mirror)\n  {}",
                        brief
                    ));
                    (String::new(), String::new(), Vec::new(), Vec::new())
                }
            }
        };
    Ok(Project {
        files: ctx.files,
        tailwind_css,
        tailwind_theme_css,
        unknown_classes,
        dropped_selectors,
        warnings: ctx.warnings,
        class_sources: ctx.class_sources,
        size_budget: ctx.size_budget,
        app,
    })
}

#[derive(Clone, Copy, PartialEq)]
enum UnitKind {
    Page,
    Component,
    Template,
}

struct ProjectCtx {
    /// npm package → which packages import it (None = main package)
    npm_usage: std::collections::BTreeMap<String, std::collections::BTreeSet<Option<String>>>,
    /// app.mist's `config.minLibVersion` — the project-wide floor units inherit
    /// unless they declare their own
    min_lib: Option<String>,
    /// app.mist's `config.sizeBudget` in bytes — opt-in M1029 threshold
    size_budget: Option<u64>,
    /// canonical store path → module info, analyzed once per project
    store_info_memo: std::cell::RefCell<std::collections::HashMap<PathBuf, Option<frontmatter::StoreModuleInfo>>>,
    /// import path → the store module's own compile error
    store_errors: std::cell::RefCell<std::collections::HashMap<String, String>>,
    /// canonical component path → is_inlinable verdict
    inlinable_memo: std::cell::RefCell<std::collections::HashMap<PathBuf, bool>>,
    seen: Vec<PathBuf>,
    files: Vec<CompiledFile>,
    classes: Vec<String>,
    /// kebab name → raw style of compiled template partials (for parent merging)
    template_styles: BTreeMap<String, String>,
    /// `<style global>` blocks hoisted out of units — appended to app.wxss
    global_styles: Vec<String>,
    /// store out_path → source file, to reject two different files landing on one path
    store_out_paths: BTreeMap<String, PathBuf>,
    class_sources: BTreeMap<String, String>,
    /// class names hand-defined in any user `<style>` block (app.mist or a
    /// unit's own) — subtracted from `unknown_classes` before M1002 fires.
    style_defined_classes: std::collections::HashSet<String>,
    warnings: Vec<String>,
    layout: Layout,
    theme: Option<String>,
}

fn compile_rec(
    path: &Path,
    kind: UnitKind,
    depth: usize,
    package: Option<&str>,
    ctx: &mut ProjectCtx,
) -> Result<(), String> {
    compile_rec_at(path, kind, depth, package, None, None, ctx)
}

/// `forced_out_path`: overrides the computed `out_path` for this unit only —
/// used for the custom tab bar, which WeChat requires at the fixed dist path
/// `custom-tab-bar/index` regardless of its source location.
fn compile_rec_at(
    path: &Path,
    kind: UnitKind,
    depth: usize,
    package: Option<&str>,
    forced_out_path: Option<&str>,
    route: Option<(&str, &str)>,
    ctx: &mut ProjectCtx,
) -> Result<(), String> {
    let canonical = path.canonicalize().map_err(|e| format!("cannot resolve {}: {}", path.display(), e))?;
    if ctx.seen.contains(&canonical) {
        return Ok(());
    }
    ctx.seen.push(canonical);

    let source = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let is_page = kind == UnitKind::Page;
    let name = match route {
        Some((page_name, _)) => page_name.to_string(),
        None if is_page => stem.clone(),
        None => wxml::kebab(&stem),
    };

    if kind == UnitKind::Template {
        let floor = ctx.min_lib.clone();
        let unit = compile_template_unit(&source, &name, floor.as_deref())
            .map_err(|e| format!("{}: {}", path.display(), e))?;
        ctx.warnings.extend(unit.warnings.iter().map(|w| format!("{}: {}", path.display(), w)));
        for c in &unit.classes {
            ctx.class_sources.entry(c.clone()).or_insert_with(|| path.display().to_string());
        }
        ctx.classes.extend(unit.classes.iter().cloned());
        ctx.style_defined_classes.extend(tailwind::harvest_style_classes(&unit.style));
        if unit.style_global && matches!(ctx.layout, Layout::Nested) {
            if !unit.style.is_empty() {
                ctx.global_styles.push(unit.style.clone());
            }
            ctx.template_styles.insert(name.clone(), String::new());
        } else {
            ctx.template_styles.insert(name.clone(), unit.style.clone());
        }
        let out_path = ctx.layout.out_path(&name, false);
        ctx.files.push(CompiledFile {
            name,
            out_path,
            is_page: false,
            package: None,
            output: unit.output,
            route_refs: Vec::new(),
            route_param: None,
        });
        return Ok(());
    }

    // decide which imports can be inlined before compiling this unit
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let store_memo = &ctx.store_info_memo;
    let store_errors = &ctx.store_errors;
    let resolver = |import_path: &str| -> Option<frontmatter::StoreModuleInfo> {
        let store_path = dir.join(import_path);
        let key = store_path.canonicalize().unwrap_or_else(|_| store_path.clone());
        if let Some(hit) = store_memo.borrow().get(&key) {
            return hit.clone();
        }
        let info = match std::fs::read_to_string(&key) {
            Ok(src) => match frontmatter::store_module_info(&src) {
                Ok(info) => Some(info),
                Err(e) => {
                    store_errors
                        .borrow_mut()
                        .insert(import_path.to_string(), format!("{}: {}", store_path.display(), e));
                    None
                }
            },
            Err(_) => None,
        };
        store_memo.borrow_mut().insert(key, info.clone());
        info
    };
    let sfc = sfc::split(&source).map_err(|e| format!("{}: {}", path.display(), e))?;
    let mut inline: Vec<String> = Vec::new();
    for import in frontmatter::mist_import_list(sfc.frontmatter) {
        let child_path = dir.join(&import.path);
        let key = child_path.canonicalize().unwrap_or(child_path);
        let cached = ctx.inlinable_memo.borrow().get(&key).copied();
        let verdict = match cached {
            Some(v) => v,
            None => {
                let v = std::fs::read_to_string(&key)
                    .is_ok_and(|child_src| is_inlinable(&child_src));
                ctx.inlinable_memo.borrow_mut().insert(key, v);
                v
            }
        };
        if verdict {
            inline.push(import.local);
        }
    }
    let floor = ctx.min_lib.clone();
    let unit = compile_unit_full_route(
        &source,
        is_page,
        &inline,
        ctx.layout,
        depth,
        &name,
        route.map(|(_, p)| p),
        floor.as_deref(),
        &resolver,
    )
    .map_err(|e| {
        let errs = ctx.store_errors.borrow();
        for (imp, store_err) in errs.iter() {
            if e.contains(&format!("'{}'", imp)) {
                return store_err.clone();
            }
        }
        format!("{}: {}", path.display(), e)
    })?;
    for c in &unit.classes {
        ctx.class_sources.entry(c.clone()).or_insert_with(|| path.display().to_string());
    }
    ctx.classes.extend(unit.classes.iter().cloned());
    ctx.style_defined_classes.extend(tailwind::harvest_style_classes(&unit.style));
    ctx.warnings.extend(unit.warnings.iter().map(|w| format!("{}: {}", path.display(), w)));
    for p in &unit.npm_packages {
        ctx.npm_usage.entry(p.clone()).or_default().insert(package.map(str::to_string));
    }

    // compile imported store modules (deduped across the project)
    for store_path in &unit.store_import_paths {
        let abs = dir.join(store_path);
        let canonical = abs
            .canonicalize()
            .map_err(|e| format!("cannot resolve {}: {}", abs.display(), e))?;
        if ctx.seen.contains(&canonical) {
            continue;
        }
        ctx.seen.push(canonical);
        let src = std::fs::read_to_string(&abs).map_err(|e| format!("cannot read {}: {}", abs.display(), e))?;
        let (js, store_info) = frontmatter::compile_store_module_vendored(
            &src,
            ctx.layout.store_rt_require(),
            ctx.layout.store_vendor_prefix(),
        )
        .map_err(|e| format!("{}: {}", abs.display(), e))?;
        for p in &store_info.npm_packages {
            ctx.npm_usage.entry(p.clone()).or_default().insert(None);
        }
        let stem = abs.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let out_path = ctx.layout.store_out_path(&stem);
        if let Some(prev) = ctx.store_out_paths.get(&out_path) {
            return Err(format!(
                "store output path collision: '{}' and '{}' both compile to {} — rename one",
                prev.display(),
                abs.display(),
                out_path
            ));
        }
        ctx.store_out_paths.insert(out_path.clone(), abs.clone());
        ctx.files.push(CompiledFile {
            name: stem,
            out_path,
            is_page: false,
            package: None,
            output: Output { wxml: String::new(), js, wxss: String::new(), json: None },
            route_refs: Vec::new(),
            route_param: None,
        });
    }

    for (local, import_path) in &unit.used_imports {
        let child_kind = if unit.used_inline_locals.contains(local) {
            UnitKind::Template
        } else {
            UnitKind::Component
        };
        compile_rec(&dir.join(import_path), child_kind, DEFAULT_DEPTH, None, ctx)?;
    }

    // inlined children render in this unit's context — merge their styles here
    let merged: Vec<String> = unit
        .used_inline_locals
        .iter()
        .filter_map(|local| {
            let path = unit.used_imports.iter().find(|(l, _)| l == local).map(|(_, p)| p)?;
            let child_stem = Path::new(path).file_stem().unwrap_or_default().to_string_lossy().to_string();
            ctx.template_styles.get(&wxml::kebab(&child_stem)).cloned()
        })
        .collect();
    let route_refs = unit.route_refs.clone();
    let hoist_global = unit.style_global && matches!(ctx.layout, Layout::Nested);
    let own_style = if hoist_global { "" } else { unit.style.as_str() };
    let mut output = unit.output;
    if !merged.is_empty() {
        let mut combined = unit.classes.clone();
        combined.extend(ctx.classes.iter().cloned());
        output.wxss = assemble_wxss(&combined, own_style, &merged, ctx.layout, depth, is_page);
    } else if hoist_global {
        output.wxss = assemble_wxss(&unit.classes, own_style, &[], ctx.layout, depth, is_page);
    }
    if hoist_global && !unit.style.is_empty() {
        ctx.global_styles.push(unit.style.clone());
    }

    let out_path = match (forced_out_path, package) {
        (Some(forced), _) => forced.to_string(),
        (None, Some(pkg)) => Layout::subpkg_out_path(pkg, &name),
        (None, None) => ctx.layout.out_path(&name, is_page),
    };
    ctx.files.push(CompiledFile {
        name,
        out_path,
        is_page,
        package: package.map(str::to_string),
        output,
        route_refs,
        route_param: route.map(|(_, p)| p.to_string()),
    });
    Ok(())
}

fn build_json(
    config: Option<&str>,
    using: &BTreeMap<String, String>,
    is_page: bool,
    style_global: bool,
) -> Result<Option<String>, String> {
    // a global-styled component must receive app/page styles or its own
    // (now app-level) rules would never reach it
    let default_isolation = if style_global { "apply-shared" } else { "isolated" };
    let mut fields: Vec<String> = Vec::new();
    if !is_page {
        fields.push("\"component\": true".to_string());
    }
    if !using.is_empty() {
        let entries: Vec<String> =
            using.iter().map(|(k, v)| format!("\"{}\": \"{}\"", k, v)).collect();
        fields.push(format!("\"usingComponents\": {{ {} }}", entries.join(", ")));
    }
    if let Some(config) = config {
        let keys = frontmatter::config_top_level_keys(config)?;
        if !is_page {
            reject_reserved_key(
                &keys,
                "component",
                "mistc marks every unit under src/components/ as a component automatically; remove the manual entry",
            )?;
        }
        if !using.is_empty() {
            reject_reserved_key(
                &keys,
                "usingComponents",
                "mistc registers imported .mist components automatically; remove the manual entry",
            )?;
        }
        if !is_page && !keys.iter().any(|k| k == "styleIsolation") {
            fields.push(format!("\"styleIsolation\": \"{}\"", default_isolation));
        }
        let json = frontmatter::config_literal_to_json(config)?;
        let inner = object_inner(&json).to_string();
        if style_global
            && !is_page
            && (inner.contains("\"styleIsolation\": \"isolated\"")
                || inner.contains("\"styleIsolation\": \"page-isolated\""))
        {
            return Err(
                "config.styleIsolation 'isolated' blocks this component's own <style global> rules — they move to app.wxss, which isolation shuts out\n  help: use 'apply-shared' or 'shared', or drop the global attribute".to_string(),
            );
        }
        if !inner.is_empty() {
            fields.push(inner);
        }
    } else if !is_page {
        fields.push(format!("\"styleIsolation\": \"{}\"", default_isolation));
    }
    if fields.is_empty() && is_page {
        return Ok(None);
    }
    Ok(Some(format!("{{ {} }}", fields.join(", "))))
}

/// M1014: reject a config key that collides with a compiler-generated JSON field.
fn reject_reserved_key(keys: &[String], reserved: &str, help: &str) -> Result<(), String> {
    if keys.iter().any(|k| k == reserved) {
        return Err(format!(
            "M1014: config key '{}' collides with a field mistc generates\n  help: {}",
            reserved, help
        ));
    }
    Ok(())
}

/// Strip exactly one pair of surrounding braces.
fn object_inner(json: &str) -> &str {
    let t = json.trim();
    let t = t.strip_prefix('{').unwrap_or(t);
    let t = t.strip_suffix('}').unwrap_or(t);
    t.trim()
}
