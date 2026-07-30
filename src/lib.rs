// Compiler internals. Public only behind the `internals` feature so tooling
// (LSP, editor plugins) can opt in; the stable surface is the re-exports below.
#[cfg(feature = "internals")]
pub mod frontmatter;
#[cfg(not(feature = "internals"))]
pub(crate) mod frontmatter;

#[cfg(feature = "internals")]
pub mod sfc;
#[cfg(not(feature = "internals"))]
pub(crate) mod sfc;

#[cfg(feature = "internals")]
pub mod tailwind;
#[cfg(not(feature = "internals"))]
pub(crate) mod tailwind;

#[cfg(feature = "internals")]
pub mod tailwind_cli;
#[cfg(not(feature = "internals"))]
pub(crate) mod tailwind_cli;

#[cfg(feature = "internals")]
pub mod template;
#[cfg(not(feature = "internals"))]
pub(crate) mod template;

#[cfg(feature = "internals")]
pub mod wxml;
#[cfg(not(feature = "internals"))]
pub(crate) mod wxml;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const RUNTIME: &str = include_str!("../runtime/mist-rt.js");

#[derive(Debug)]
pub struct Output {
    pub wxml: String,
    pub js: String,
    pub wxss: String,
    pub json: Option<String>,
}

pub struct Unit {
    pub output: Output,
    pub used_imports: Vec<(String, String)>,
    pub used_inline_locals: Vec<String>,
    pub classes: Vec<String>,
    pub style: String,
    pub store_import_paths: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Layout {
    Flat,
    Nested,
}

impl Layout {
    fn rt_require(&self) -> &'static str {
        match self {
            Layout::Flat => "./mist-rt.js",
            Layout::Nested => "../../mist-rt.js",
        }
    }
    fn tw_import(&self) -> &'static str {
        match self {
            Layout::Flat => "./tw-shared.wxss",
            Layout::Nested => "../../tw-shared.wxss",
        }
    }
    fn tw_theme_import(&self) -> &'static str {
        match self {
            Layout::Flat => "./tw-theme.wxss",
            Layout::Nested => "../../tw-theme.wxss",
        }
    }
    fn component_ref(&self, kebab: &str) -> String {
        match self {
            Layout::Flat => format!("./{}", kebab),
            Layout::Nested => format!("../../components/{}/{}", kebab, kebab),
        }
    }
    fn template_ref(&self, kebab: &str) -> String {
        match self {
            Layout::Flat => format!("./{}.wxml", kebab),
            Layout::Nested => format!("../../components/{}/{}.wxml", kebab, kebab),
        }
    }
    fn out_path(&self, name: &str, is_page: bool) -> String {
        match self {
            Layout::Flat => name.to_string(),
            Layout::Nested if is_page => format!("pages/{}/{}", name, name),
            Layout::Nested => format!("components/{}/{}", name, name),
        }
    }
    fn store_require(&self, stem: &str) -> String {
        match self {
            Layout::Flat => format!("./{}.js", stem),
            Layout::Nested => format!("../../stores/{}.js", stem),
        }
    }
    fn store_out_path(&self, stem: &str) -> String {
        match self {
            Layout::Flat => stem.to_string(),
            Layout::Nested => format!("stores/{}", stem),
        }
    }
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
    compile_unit_full(source, is_page, &[], Layout::Flat, &|_| None)
}

fn compile_unit_full(
    source: &str,
    is_page: bool,
    inline: &[String],
    layout: Layout,
    resolve_store: &dyn Fn(&str) -> Option<frontmatter::StoreModuleInfo>,
) -> Result<Unit, String> {
    let sfc = sfc::split(source)?;
    let mut analysis = frontmatter::analyze_with_stores_bound(sfc.frontmatter, resolve_store, sfc.frontmatter_line, Some(sfc.template))?;
    for si in &mut analysis.store_imports {
        let stem = Path::new(&si.path).file_stem().unwrap_or_default().to_string_lossy().to_string();
        si.require_path = layout.store_require(&stem);
    }

    let mut reactive: Vec<String> = analysis.states.iter().map(|s| s.name.clone()).collect();
    reactive.extend(analysis.deriveds.iter().map(|d| d.name.clone()));
    reactive.extend(analysis.store_imports.iter().flat_map(|si| si.stores.iter().cloned()));

    let component_locals: Vec<String> = analysis
        .imports
        .iter()
        .map(|i| i.local.clone())
        .filter(|l| !inline.contains(l))
        .collect();

    let nodes = template::parse_at(sfc.template, sfc.template_line)?;
    let wxml_out = wxml::emit(&nodes, &reactive, &component_locals, inline)?;

    // wx:key per derived, resolved from template loops rendering that derived
    let loops = template::for_lists(&nodes);
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

    let hoists = frontmatter::hoisted_deriveds(&analysis, &wxml_out.hoisted, &wxml_out.for_hoists)?;
    let mut derived_keys = derived_keys;
    for (d, key) in hoists {
        analysis.deriveds.push(d);
        derived_keys.push(key);
    }

    let multiple_slots = !is_page && template::has_named_slot(&nodes);
    let js = frontmatter::emit_js(
        &analysis,
        &wxml_out.handlers,
        &derived_keys,
        is_page,
        multiple_slots,
        layout.rt_require(),
        &wxml_out.vbinds,
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

    let using: BTreeMap<String, String> = wxml_out
        .used_components
        .iter()
        .filter_map(|l| import_path_of(l).map(|p| (l, p)))
        .map(|(local, path)| {
            let stem = Path::new(&path).file_stem().unwrap_or_default().to_string_lossy().to_string();
            (wxml::kebab(local), layout.component_ref(&wxml::kebab(&stem)))
        })
        .collect();

    let json = build_json(analysis.config.as_deref(), &using, is_page)?;

    let mut wxml = String::new();
    for local in &wxml_out.used_inline {
        if let Some(path) = import_path_of(local) {
            let stem = Path::new(&path).file_stem().unwrap_or_default().to_string_lossy().to_string();
            wxml.push_str(&format!("<import src=\"{}\" />\n", layout.template_ref(&wxml::kebab(&stem))));
        }
    }
    wxml.push_str(&wxml_out.wxml);

    let classes = tailwind::extract_classes(&nodes);
    let style = sfc.style.unwrap_or("").to_string();
    let wxss = assemble_wxss(&classes, &style, &[], layout, is_page);

    let store_import_paths: Vec<String> =
        analysis.store_imports.iter().map(|si| si.path.clone()).collect();

    Ok(Unit {
        output: Output { wxml, js, wxss, json },
        used_imports,
        used_inline_locals: wxml_out.used_inline,
        classes,
        style,
        store_import_paths,
    })
}

fn compile_template_unit(source: &str, name: &str) -> Result<Unit, String> {
    let sfc = sfc::split(source)?;
    let nodes = template::parse_at(sfc.template, sfc.template_line)?;
    let wxml_out = wxml::emit(&nodes, &[], &[], &[])?;
    let body: String = wxml_out.wxml.lines().map(|l| format!("  {}\n", l)).collect();
    let wxml = format!("<template name=\"{}\">\n{}</template>\n", name, body);
    let classes = tailwind::extract_classes(&nodes);
    let style = sfc.style.unwrap_or("").to_string();
    Ok(Unit {
        output: Output { wxml, js: String::new(), wxss: String::new(), json: None },
        used_imports: Vec::new(),
        used_inline_locals: Vec::new(),
        classes,
        style,
        store_import_paths: Vec::new(),
    })
}

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
        && analysis.config.is_none()
        && !template::has_slot(&nodes)
        && !template::has_events(&nodes)
}

fn assemble_wxss(
    classes: &[String],
    own_style: &str,
    merged_styles: &[String],
    layout: Layout,
    is_page: bool,
) -> String {
    let mut wxss = String::new();
    // theme sheet is legal only in page WXSS, so components never import it
    if !classes.is_empty() {
        if is_page {
            wxss.push_str(&format!("@import \"{}\";\n", layout.tw_theme_import()));
        }
        wxss.push_str(&format!("@import \"{}\";\n", layout.tw_import()));
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
    pub name: String,
    pub out_path: String,
    pub is_page: bool,
    pub output: Output,
}

#[derive(Debug)]
pub struct Project {
    pub files: Vec<CompiledFile>,
    pub tailwind_css: String,
    /// `page { ... }` theme variables — written to `tw-theme.wxss`, imported by pages only
    pub tailwind_theme_css: String,
    pub unknown_classes: Vec<String>,
    /// rules removed because WXSS cannot express their selector
    pub dropped_selectors: Vec<String>,
    /// present for directory builds: app.js/app.json/app.wxss
    pub app: Option<AppShell>,
}

/// Utilities are generated by the Tailwind v4 CLI and post-processed for WXSS.
pub fn compile_project(entry: &Path) -> Result<Project, String> {
    let mut ctx = new_project_ctx(Layout::Flat);
    compile_rec(entry, UnitKind::Page, &mut ctx)?;
    finish_project(ctx, None)
}

/// `app.js`/`app.json`/`app.wxss` at the root.
pub fn compile_project_dir(src: &Path) -> Result<Project, String> {
    let app_path = src.join("app.mist");
    let app_source = std::fs::read_to_string(&app_path)
        .map_err(|e| format!("cannot read {}: {}", app_path.display(), e))?;

    let pages_dir = src.join("pages");
    let mut page_paths: Vec<PathBuf> = std::fs::read_dir(&pages_dir)
        .map_err(|e| format!("cannot read {}: {}", pages_dir.display(), e))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "mist"))
        .collect();
    page_paths.sort();
    page_paths.sort_by_key(|p| p.file_stem().map(|s| s != "index").unwrap_or(true));
    if page_paths.is_empty() {
        return Err(format!("no pages found in {}", pages_dir.display()));
    }

    let mut ctx = new_project_ctx(Layout::Nested);
    for page in &page_paths {
        compile_rec(page, UnitKind::Page, &mut ctx)?;
    }

    let app = compile_app(&app_source, &ctx)?;
    finish_project(ctx, Some(app))
}

fn new_project_ctx(layout: Layout) -> ProjectCtx {
    ProjectCtx {
        seen: Vec::new(),
        files: Vec::new(),
        classes: Vec::new(),
        template_styles: BTreeMap::new(),
        store_out_paths: BTreeMap::new(),
        layout,
    }
}

fn compile_app(source: &str, ctx: &ProjectCtx) -> Result<AppShell, String> {
    let sfc = sfc::split(source).map_err(|e| format!("app.mist: {}", e))?;
    let analysis = frontmatter::analyze(sfc.frontmatter).map_err(|e| format!("app.mist: {}", e))?;
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

    let pages: Vec<String> = ctx
        .files
        .iter()
        .filter(|f| f.is_page)
        .map(|f| format!("\"{}\"", f.out_path))
        .collect();
    let mut fields = vec![format!("\"pages\": [{}]", pages.join(", "))];
    if let Some(config) = analysis.config.as_deref() {
        let json = frontmatter::config_literal_to_json(config).map_err(|e| format!("app.mist: {}", e))?;
        let inner = object_inner(&json).to_string();
        if !inner.is_empty() {
            fields.push(inner);
        }
    }
    fields.push("\"sitemapLocation\": \"sitemap.json\"".to_string());
    let json = format!("{{ {} }}", fields.join(", "));

    let wxss = sfc.style.unwrap_or("").to_string();
    Ok(AppShell { js, json, wxss })
}

#[derive(Debug)]
pub struct AppShell {
    pub js: String,
    pub json: String,
    pub wxss: String,
}

fn finish_project(mut ctx: ProjectCtx, app: Option<AppShell>) -> Result<Project, String> {
    ctx.classes.sort();
    ctx.classes.dedup();
    let (tailwind_css, tailwind_theme_css, unknown_classes, dropped_selectors) =
        if ctx.classes.is_empty() {
            (String::new(), String::new(), Vec::new(), Vec::new())
        } else {
            let result = tailwind_cli::generate(&ctx.classes)?;
            let unknown: Vec<String> = ctx
                .classes
                .iter()
                .filter(|c| {
                    !result.css.contains(&format!(".{}", tailwind::sanitize(c)))
                        && !result.dropped_selectors.iter().any(|d| d.contains(c.as_str()))
                })
                .cloned()
                .collect();
            (result.css, result.theme_css, unknown, result.dropped_selectors)
        };
    Ok(Project {
        files: ctx.files,
        tailwind_css,
        tailwind_theme_css,
        unknown_classes,
        dropped_selectors,
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
    seen: Vec<PathBuf>,
    files: Vec<CompiledFile>,
    classes: Vec<String>,
    template_styles: BTreeMap<String, String>,
    store_out_paths: BTreeMap<String, PathBuf>,
    layout: Layout,
}

fn compile_rec(path: &Path, kind: UnitKind, ctx: &mut ProjectCtx) -> Result<(), String> {
    let canonical = path.canonicalize().map_err(|e| format!("cannot resolve {}: {}", path.display(), e))?;
    if ctx.seen.contains(&canonical) {
        return Ok(());
    }
    ctx.seen.push(canonical);

    let source = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let is_page = kind == UnitKind::Page;
    let name = if is_page { stem.clone() } else { wxml::kebab(&stem) };

    if kind == UnitKind::Template {
        let unit = compile_template_unit(&source, &name).map_err(|e| format!("{}: {}", path.display(), e))?;
        ctx.classes.extend(unit.classes.iter().cloned());
        ctx.template_styles.insert(name.clone(), unit.style.clone());
        let out_path = ctx.layout.out_path(&name, false);
        ctx.files.push(CompiledFile { name, out_path, is_page: false, output: unit.output });
        return Ok(());
    }

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let resolver = |import_path: &str| -> Option<frontmatter::StoreModuleInfo> {
        let store_path = dir.join(import_path);
        let src = std::fs::read_to_string(&store_path).ok()?;
        frontmatter::store_module_info(&src).ok()
    };
    let sfc = sfc::split(&source).map_err(|e| format!("{}: {}", path.display(), e))?;
    let analysis = frontmatter::analyze_with_stores(sfc.frontmatter, &resolver, sfc.frontmatter_line)
        .map_err(|e| format!("{}: {}", path.display(), e))?;
    let mut inline: Vec<String> = Vec::new();
    for import in &analysis.imports {
        let child_path = dir.join(&import.path);
        if let Ok(child_src) = std::fs::read_to_string(&child_path) {
            if is_inlinable(&child_src) {
                inline.push(import.local.clone());
            }
        }
    }
    let unit = compile_unit_full(&source, is_page, &inline, ctx.layout, &resolver)
        .map_err(|e| format!("{}: {}", path.display(), e))?;
    ctx.classes.extend(unit.classes.iter().cloned());

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
        let (js, _) = frontmatter::compile_store_module(&src, ctx.layout.store_rt_require())
            .map_err(|e| format!("{}: {}", abs.display(), e))?;
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
            output: Output { wxml: String::new(), js, wxss: String::new(), json: None },
        });
    }

    for (local, import_path) in &unit.used_imports {
        let child_kind = if unit.used_inline_locals.contains(local) {
            UnitKind::Template
        } else {
            UnitKind::Component
        };
        compile_rec(&dir.join(import_path), child_kind, ctx)?;
    }

    let merged: Vec<String> = unit
        .used_inline_locals
        .iter()
        .filter_map(|local| {
            let path = unit.used_imports.iter().find(|(l, _)| l == local).map(|(_, p)| p)?;
            let child_stem = Path::new(path).file_stem().unwrap_or_default().to_string_lossy().to_string();
            ctx.template_styles.get(&wxml::kebab(&child_stem)).cloned()
        })
        .collect();
    let mut output = unit.output;
    if !merged.is_empty() {
        let mut combined = unit.classes.clone();
        combined.extend(ctx.classes.iter().cloned());
        output.wxss = assemble_wxss(&combined, &unit.style, &merged, ctx.layout, is_page);
    }

    let out_path = ctx.layout.out_path(&name, is_page);
    ctx.files.push(CompiledFile { name, out_path, is_page, output });
    Ok(())
}

fn build_json(
    config: Option<&str>,
    using: &BTreeMap<String, String>,
    is_page: bool,
) -> Result<Option<String>, String> {
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
        let json = frontmatter::config_literal_to_json(config)?;
        let inner = object_inner(&json).to_string();
        if !inner.is_empty() {
            fields.push(inner);
        }
    }
    if fields.is_empty() && is_page {
        return Ok(None);
    }
    Ok(Some(format!("{{ {} }}", fields.join(", "))))
}

fn object_inner(json: &str) -> &str {
    let t = json.trim();
    let t = t.strip_prefix('{').unwrap_or(t);
    let t = t.strip_suffix('}').unwrap_or(t);
    t.trim()
}
