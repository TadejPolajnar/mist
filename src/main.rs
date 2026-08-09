use std::fs;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::sync::mpsc;
use std::time::Duration;

use clap::{Parser, Subcommand};
use notify::{RecursiveMode, Watcher};

#[derive(Parser)]
#[command(
    name = "mistc",
    version,
    about = "Compile .mist single-file components to native WeChat Mini Program code"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Compile a project directory or a single .mist entry file")]
    Build {
        #[arg(help = "Project src directory (with app.mist + pages/) or a single .mist file")]
        input: PathBuf,
        #[arg(short, long, default_value = "dist", help = "Output directory")]
        output: PathBuf,
        #[arg(long, help = "Single-file builds: also emit a minimal DevTools-openable app shell")]
        app: bool,
        #[arg(long, help = "Rebuild automatically when source files change")]
        watch: bool,
    },
    #[command(about = "Scaffold a new mist project")]
    Init {
        #[arg(help = "Project directory to create")]
        name: String,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Build { input, output, app, watch } => {
            let ok = run_build(&input, &output, app);
            if watch {
                watch_loop(&input, &output, app);
            } else if !ok {
                exit(1);
            }
        }
        Command::Init { name } => {
            if let Err(e) = init_project(&name) {
                eprintln!("error: {}", e);
                exit(1);
            }
        }
    }
}

fn run_build(input: &Path, outdir: &Path, emit_app: bool) -> bool {
    let project = if input.is_dir() {
        mistc::compile_project_dir(input)
    } else {
        mistc::compile_project(input)
    };
    let project = match project {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            return false;
        }
    };
    let files = &project.files;

    fs::create_dir_all(outdir).expect("cannot create output dir");
    let mut written: Vec<String> = Vec::new();
    let emit = |written: &mut Vec<String>, rel: String, content: &str| {
        let p = outdir.join(&rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).expect("cannot create output dir");
        }
        fs::write(p, content).unwrap();
        written.push(rel);
    };
    if !project.tailwind_css.is_empty() {
        emit(&mut written, "tw-shared.wxss".into(), &project.tailwind_css);
        // pages import this unconditionally when they use classes — write even if empty
        emit(&mut written, "tw-theme.wxss".into(), &project.tailwind_theme_css);
    }
    for w in &project.warnings {
        eprintln!("warning {}", w);
    }
    for c in &project.unknown_classes {
        match project.class_sources.get(c) {
            Some(src) => eprintln!("warning M1002: unknown class '{}' in {} (no WXSS generated)", c, src),
            None => eprintln!("warning M1002: unknown class '{}' (no WXSS generated)", c),
        }
    }
    for s in &project.dropped_selectors {
        eprintln!("warning M1006: dropped rule — WXSS cannot express selector '{}'", s.trim());
    }
    for f in files {
        if !f.output.wxml.is_empty() {
            emit(&mut written, format!("{}.wxml", f.out_path), &f.output.wxml);
        }
        if !f.output.js.is_empty() {
            emit(&mut written, format!("{}.js", f.out_path), &f.output.js);
        }
        if !f.output.wxss.is_empty() {
            emit(&mut written, format!("{}.wxss", f.out_path), &f.output.wxss);
        }
        if let Some(json) = &f.output.json {
            emit(&mut written, format!("{}.json", f.out_path), json);
        }
        let tag = if f.is_page {
            " (page)"
        } else if f.output.js.is_empty() {
            " (inlined template)"
        } else if f.output.wxml.is_empty() {
            " (store)"
        } else {
            ""
        };
        println!("  {}{}", f.out_path, tag);
    }
    emit(&mut written, "mist-rt.js".into(), mistc::RUNTIME);

    if let Some(app) = &project.app {
        emit(&mut written, "app.js".into(), &app.js);
        emit(&mut written, "app.json".into(), &app.json);
        emit(&mut written, "app.wxss".into(), &app.wxss);
        let sitemap = fs::read_to_string(input.join("sitemap.json"))
            .unwrap_or_else(|_| "{ \"rules\": [] }\n".to_string());
        emit(&mut written, "sitemap.json".into(), &sitemap);
    } else if emit_app {
        let page = files
            .iter()
            .find(|f| f.is_page)
            .map(|f| f.out_path.clone())
            .unwrap_or_else(|| "index".to_string());
        emit(
            &mut written,
            "app.js".into(),
            "const rt = require('./mist-rt.js');\nrt.observePerf();\nApp({ __perf: rt.perfEntries });\n",
        );
        emit(
            &mut written,
            "app.json".into(),
            &format!("{{ \"pages\": [\"{}\"], \"sitemapLocation\": \"sitemap.json\" }}\n", page),
        );
        let sitemap_root = input.parent().unwrap_or_else(|| Path::new("."));
        let sitemap = fs::read_to_string(sitemap_root.join("sitemap.json"))
            .unwrap_or_else(|_| "{ \"rules\": [] }\n".to_string());
        emit(&mut written, "sitemap.json".into(), &sitemap);
        emit(&mut written, "app.wxss".into(), "");
        emit(
            &mut written,
            "project.config.json".into(),
            "{ \"appid\": \"touristappid\", \"compileType\": \"miniprogram\", \"setting\": { \"es6\": true } }\n",
        );
    }
    if input.is_dir() {
        copy_dir_verbatim(&input.join("assets"), outdir, "assets", &mut written);

        let theme_root = input;
        if let Ok(theme) = fs::read_to_string(theme_root.join("theme.json")) {
            emit(&mut written, "theme.json".into(), &theme);
        }

        copy_dir_verbatim(&input.join("workers"), outdir, "workers", &mut written);

        write_routes_dts(input, files);
    } else {
        let theme_root = input.parent().unwrap_or_else(|| Path::new("."));
        if let Ok(theme) = fs::read_to_string(theme_root.join("theme.json")) {
            emit(&mut written, "theme.json".into(), &theme);
        }
    }

    prune_stale(outdir, &written);
    println!("compiled {} file(s) → {}/", files.len(), outdir.display());
    true
}

fn copy_dir_verbatim(src_dir: &Path, outdir: &Path, rel_prefix: &str, written: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(src_dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else { continue };
        if meta.file_type().is_symlink() {
            continue;
        }
        let rel = format!("{}/{}", rel_prefix, name);
        if path.is_dir() {
            copy_dir_verbatim(&path, outdir, &rel, written);
        } else {
            let dest = outdir.join(&rel);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).expect("cannot create output dir");
            }
            fs::copy(&path, &dest).expect("cannot copy asset");
            written.push(rel);
        }
    }
}

/// Writes `mist-routes.d.ts` next to an existing `mist.d.ts` (init-scaffolded
/// projects only — never in `dist/`, never recorded in the manifest, so it
/// survives regardless of what mistc emits). `src` is the project's `src/`
/// directory; the file lives at its parent (the project root, sibling of
/// `mist.d.ts`). Skipped silently when `mist.d.ts` isn't there. Change-compared
/// so `--watch` doesn't rewrite it (and retrigger the watcher) every rebuild.
fn write_routes_dts(src: &Path, files: &[mistc::CompiledFile]) {
    let root = src.parent().unwrap_or(src);
    let marker = root.join("mist.d.ts");
    if !marker.is_file() {
        return;
    }
    let mut routes: Vec<String> = files.iter().filter(|f| f.is_page).map(|f| format!("/{}", f.out_path)).collect();
    routes.sort();
    routes.dedup();
    let route_union = if routes.is_empty() {
        "string".to_string()
    } else {
        routes.iter().map(|r| format!("\"{}\"", r)).collect::<Vec<_>>().join(" | ")
    };
    let content = format!(
        "// generated by mistc — do not edit\ndeclare module 'mist' {{\n  export type Route = {};\n  export function navigate(route: Route, params?: Record<string, string | number | boolean>): void;\n  export namespace navigate {{\n    function replace(route: Route, params?: Record<string, string | number | boolean>): void;\n    function back(delta?: number): void;\n    function switchTab(route: Route): void;\n  }}\n}}\n",
        route_union
    );
    let dest = root.join("mist-routes.d.ts");
    if fs::read_to_string(&dest).map(|existing| existing == content).unwrap_or(false) {
        return;
    }
    let _ = fs::write(dest, content);
}

fn prune_stale(outdir: &Path, written: &[String]) {
    let manifest = outdir.join(".mist-manifest");
    if let Ok(prev) = fs::read_to_string(&manifest) {
        for line in prev.lines() {
            if line.is_empty() || written.iter().any(|w| w == line) {
                continue;
            }
            if fs::remove_file(outdir.join(line)).is_ok() {
                println!("  removed stale {}", line);
            }
            let mut parent = Path::new(line).parent();
            while let Some(p) = parent {
                if p.as_os_str().is_empty() || fs::remove_dir(outdir.join(p)).is_err() {
                    break;
                }
                parent = p.parent();
            }
        }
    }
    let mut sorted: Vec<&String> = written.iter().collect();
    sorted.sort();
    let body: String = sorted.iter().map(|w| format!("{}\n", w)).collect();
    let _ = fs::write(&manifest, body);
}

fn watch_loop(input: &Path, outdir: &Path, emit_app: bool) -> ! {
    let watch_root = if input.is_dir() {
        input.to_path_buf()
    } else {
        input.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
    };
    let out_abs = outdir.canonicalize().ok();
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            let relevant = event.paths.iter().any(|p| {
                let ext = p.extension().and_then(|e| e.to_str());
                let name = p.file_name().and_then(|n| n.to_str());
                matches!(ext, Some("mist") | Some("ts"))
                    || p.components().any(|c| {
                        let c = c.as_os_str();
                        c == "assets" || c == "workers"
                    })
                    || matches!(name, Some("theme.json") | Some("sitemap.json"))
            });
            if relevant {
                let _ = tx.send(event.paths);
            }
        }
    })
    .expect("cannot create file watcher");
    watcher
        .watch(&watch_root, RecursiveMode::Recursive)
        .expect("cannot watch source directory");
    println!("watching {} for changes… (ctrl-c to quit)", watch_root.display());
    loop {
        let Ok(mut paths) = rx.recv() else { continue };
        while let Ok(more) = rx.recv_timeout(Duration::from_millis(120)) {
            paths.extend(more);
        }
        if let Some(out) = &out_abs {
            paths.retain(|p| p.canonicalize().map(|c| !c.starts_with(out)).unwrap_or(true));
            if paths.is_empty() {
                continue;
            }
        }
        println!("— rebuilding");
        run_build(input, outdir, emit_app);
    }
}

fn init_project(name: &str) -> Result<(), String> {
    let root = PathBuf::from(name);
    if root.exists() {
        return Err(format!("'{}' already exists", name));
    }
    let title = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string());
    let src = root.join("src");
    fs::create_dir_all(src.join("pages")).map_err(|e| e.to_string())?;

    let app = format!(
        "---\nimport {{ onLaunch }} from 'mist'\n\nexport const config = {{\n  window: {{\n    navigationBarTitleText: '{}',\n  }},\n}}\n\nonLaunch(() => {{\n  console.log('{} launched')\n}})\n---\n\n<style>\npage {{ background: #f9fafb; }}\n</style>\n",
        title, title
    );
    let index = "---\nimport { state, derived } from 'mist'\n\nexport const config = { navigationBarTitleText: 'Todos' }\n\nconst todos = state([\n  { id: 1, title: 'Try mist', done: false },\n  { id: 2, title: 'Read the docs', done: false },\n])\nconst open = derived(() => todos.value.filter(t => !t.done))\n\nfunction toggle(id) {\n  const i = todos.value.findIndex(t => t.id === id)\n  todos.value[i].done = !todos.value[i].done\n}\n\nfunction add() {\n  todos.value.push({ id: todos.value.length + 1, title: `Todo ${todos.value.length + 1}`, done: false })\n}\n---\n<div class=\"p-4 flex flex-col gap-2\">\n  <span class=\"text-xl font-bold\">{open.value.length} open</span>\n  {todos.value.map(t => (\n    <div key={t.id} class=\"flex gap-2 p-2 bg-white rounded\" onTap={() => toggle(t.id)}>\n      <span>{t.done ? '✓' : '○'}</span>\n      <span>{t.title}</span>\n    </div>\n  ))}\n  <button class=\"rounded bg-blue-500 text-white p-2\" onTap={add}>Add todo</button>\n</div>\n";
    fs::write(src.join("app.mist"), app).map_err(|e| e.to_string())?;
    fs::write(src.join("pages").join("index.mist"), index).map_err(|e| e.to_string())?;
    fs::write(
        root.join("project.config.json"),
        "{\n  \"appid\": \"touristappid\",\n  \"compileType\": \"miniprogram\",\n  \"miniprogramRoot\": \"dist/\",\n  \"setting\": { \"es6\": true }\n}\n",
    )
    .map_err(|e| e.to_string())?;
    fs::write(root.join(".gitignore"), "dist/\nnode_modules/\n").map_err(|e| e.to_string())?;
    fs::write(root.join("mist.d.ts"), include_str!("../types/mist.d.ts"))
        .map_err(|e| e.to_string())?;
    fs::write(
        root.join("tsconfig.json"),
        "{\n  \"compilerOptions\": {\n    \"strict\": true,\n    \"noEmit\": true,\n    \"target\": \"es2020\",\n    \"lib\": [\"es2020\"],\n    \"module\": \"esnext\",\n    \"moduleResolution\": \"bundler\",\n    \"types\": [\"miniprogram-api-typings\"]\n  },\n  \"include\": [\"src/**/*.ts\", \"mist.d.ts\"]\n}\n",
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        root.join("package.json"),
        format!(
            "{{\n  \"name\": \"{}\",\n  \"private\": true,\n  \"devDependencies\": {{\n    \"miniprogram-api-typings\": \"^4\"\n  }}\n}}\n",
            title.to_lowercase().replace(' ', "-")
        ),
    )
    .map_err(|e| e.to_string())?;

    println!("created {}/", name);
    println!("  src/app.mist");
    println!("  src/pages/index.mist");
    println!("  project.config.json");
    println!("  mist.d.ts + tsconfig.json   (editor types for 'mist')");
    println!();
    println!("next:");
    println!("  cd {} && mistc build src --watch", name);
    println!("  npm install                  # optional: wx.* types for your editor");
    println!("  WeChat DevTools → Import Project → select {}/", name);
    Ok(())
}
