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
    #[command(about = "Compile src/ and run tests/*.test.js in a Node harness")]
    Test {
        #[arg(
            default_value = ".",
            help = "Project root containing src/ and tests/ (not the src dir itself)"
        )]
        dir: PathBuf,
        #[arg(long, help = "Only run test files (with --snapshots: emitted paths) containing this substring")]
        filter: Option<String>,
        #[arg(long, default_value_t = 30, help = "Per-file timeout in seconds")]
        timeout: u64,
        #[arg(long, help = "Rerun the tests when src/ or tests/ files change")]
        watch: bool,
        #[arg(long, help = "Diff compiled output against snapshots/ goldens (first run writes them)")]
        snapshots: bool,
        #[arg(long, help = "Rewrite the snapshots/ goldens to match current output (implies --snapshots)")]
        update: bool,
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
        Command::Test { dir, filter, timeout, watch, snapshots, update } => {
            if snapshots || update {
                if watch {
                    eprintln!("error: --snapshots does not combine with --watch — snapshot runs are one-shot");
                    exit(1);
                }
                match run_snapshots(&dir, update, filter.as_deref()) {
                    Ok(true) => return,
                    Ok(false) => exit(1),
                    Err(e) => {
                        eprintln!("error: {}", e);
                        exit(1);
                    }
                }
            }
            if watch {
                test_watch_loop(&dir, filter.as_deref(), timeout);
            }
            match run_tests(&dir, filter.as_deref(), timeout) {
                Ok(true) => {}
                Ok(false) => exit(1),
                Err(e) => {
                    eprintln!("error: {}", e);
                    exit(1);
                }
            }
        }
    }
}

fn run_node_test(
    runner: &Path,
    test: &Path,
    dist: &Path,
    timeout_secs: u64,
) -> Result<(bool, String, String), String> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = std::process::Command::new("node")
        .arg(runner)
        .arg(test)
        .env("MIST_DIST", dist)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "node is required for `mistc test` but was not found".to_string())?;
    let mut out_pipe = child.stdout.take().unwrap();
    let mut err_pipe = child.stderr.take().unwrap();
    let out_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = out_pipe.read_to_string(&mut s);
        s
    });
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = err_pipe.read_to_string(&mut s);
        s
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    let stdout = out_thread.join().unwrap_or_default();
    let stderr = err_thread.join().unwrap_or_default();
    match status {
        Some(st) => Ok((st.success(), stdout, stderr)),
        None => {
            let note = format!("timed out after {}s", timeout_secs);
            let stderr = if stderr.trim().is_empty() { note } else { format!("{}\n{}", stderr, note) };
            Ok((false, stdout, stderr))
        }
    }
}

fn test_watch_loop(dir: &Path, filter: Option<&str>, timeout_secs: u64) -> ! {
    if !dir.join("src").join("app.mist").is_file() || !dir.join("tests").is_dir() {
        eprintln!(
            "error: {} has no src/app.mist and tests/ — run from the project root",
            dir.display()
        );
        exit(1);
    }
    let run = |label: &str| {
        println!("{}", label);
        if let Err(e) = run_tests(dir, filter, timeout_secs) {
            eprintln!("error: {}", e);
        }
    };
    run("running tests…");
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            let relevant = event.paths.iter().any(|p| {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let ext = p.extension().and_then(|e| e.to_str());
                matches!(ext, Some("mist") | Some("ts")) || name.ends_with(".test.js")
            });
            if relevant {
                let _ = tx.send(());
            }
        }
    })
    .expect("cannot create file watcher");
    for sub in ["src", "tests"] {
        watcher
            .watch(&dir.join(sub), RecursiveMode::Recursive)
            .expect("cannot watch project directory");
    }
    println!(
        "watching {} and {} for changes… (ctrl-c to quit)",
        dir.join("src").display(),
        dir.join("tests").display()
    );
    loop {
        let Ok(()) = rx.recv() else { continue };
        while rx.recv_timeout(Duration::from_millis(120)).is_ok() {}
        run("— rerunning tests");
    }
}

fn collect_files(root: &Path, base: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(root) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            collect_files(&p, base, out);
        } else if let Ok(rel) = p.strip_prefix(base) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn first_diff(golden: &str, current: &str) -> String {
    let g: Vec<&str> = golden.lines().collect();
    let c: Vec<&str> = current.lines().collect();
    let mut i = 0;
    while i < g.len() && i < c.len() && g[i] == c[i] {
        i += 1;
    }
    let mut out = format!("    first difference at line {}:\n", i + 1);
    for line in g.iter().skip(i).take(3) {
        out.push_str(&format!("    - {}\n", line));
    }
    for line in c.iter().skip(i).take(3) {
        out.push_str(&format!("    + {}\n", line));
    }
    out
}

fn run_snapshots(dir: &Path, update: bool, filter: Option<&str>) -> Result<bool, String> {
    let src = dir.join("src");
    if !src.join("app.mist").is_file() {
        return Err(format!("{} has no src/app.mist — run from the project root", dir.display()));
    }
    let out = std::env::temp_dir().join(format!("mist-snap-{}", std::process::id()));
    let _ = fs::remove_dir_all(&out);
    if !run_build_opt(&src, &out, false, true) {
        let _ = fs::remove_dir_all(&out);
        return Err("build failed".to_string());
    }
    let mut emitted = Vec::new();
    collect_files(&out, &out, &mut emitted);
    emitted.sort();
    if let Some(f) = filter {
        emitted.retain(|p| p.contains(f));
    }

    let snapdir = dir.join("snapshots");
    let mut goldens = Vec::new();
    if snapdir.is_dir() {
        collect_files(&snapdir, &snapdir, &mut goldens);
        goldens.sort();
    }
    let first_run = goldens.is_empty();
    if let Some(f) = filter {
        if emitted.is_empty() && !goldens.iter().any(|p| p.contains(f)) {
            let _ = fs::remove_dir_all(&out);
            return Err(format!("no emitted file or golden matches --filter '{}'", f));
        }
    }
    if first_run || update {
        let write = || -> Result<(), String> {
            for rel in &goldens {
                if filter.is_none() && !emitted.iter().any(|e| e == rel) {
                    let _ = fs::remove_file(snapdir.join(rel));
                    let mut parent = Path::new(rel).parent();
                    while let Some(p) = parent {
                        if p.as_os_str().is_empty() || fs::remove_dir(snapdir.join(p)).is_err() {
                            break;
                        }
                        parent = p.parent();
                    }
                    println!("removed stale snapshot {}", rel);
                }
            }
            for rel in &emitted {
                let dest = snapdir.join(rel);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                fs::copy(out.join(rel), &dest).map_err(|e| e.to_string())?;
            }
            Ok(())
        };
        let res = write();
        let _ = fs::remove_dir_all(&out);
        res?;
        let label = if first_run { "written (first run)" } else { "updated" };
        println!("{} snapshot(s) {} in {}", emitted.len(), label, snapdir.display());
        return Ok(true);
    }

    if let Some(f) = filter {
        goldens.retain(|p| p.contains(f));
    }
    let mut drift = 0;
    for rel in &emitted {
        let golden_path = snapdir.join(rel);
        if !golden_path.is_file() {
            println!("ADDED   {} (no golden — run --update to accept)", rel);
            drift += 1;
            continue;
        }
        let golden = fs::read(&golden_path).unwrap_or_default();
        let current = fs::read(out.join(rel)).unwrap_or_default();
        if golden != current {
            println!("CHANGED {}", rel);
            match (std::str::from_utf8(&golden), std::str::from_utf8(&current)) {
                (Ok(g), Ok(c)) => print!("{}", first_diff(g, c)),
                _ => println!("    binary contents differ"),
            }
            drift += 1;
        }
    }
    for rel in &goldens {
        if !emitted.iter().any(|e| e == rel) {
            println!("REMOVED {} (golden exists but nothing was emitted)", rel);
            drift += 1;
        }
    }
    let _ = fs::remove_dir_all(&out);
    if drift == 0 {
        println!("{} snapshot(s) match", emitted.len());
        Ok(true)
    } else {
        println!("{} file(s) drifted — review, then `mistc test --update` to accept", drift);
        Ok(false)
    }
}

fn run_tests(dir: &Path, filter: Option<&str>, timeout_secs: u64) -> Result<bool, String> {
    let src = dir.join("src");
    if !src.join("app.mist").is_file() {
        return Err(format!("{} has no src/app.mist — run from the project root", dir.display()));
    }
    let tests_dir = dir.join("tests");
    let mut all_tests: Vec<PathBuf> = fs::read_dir(&tests_dir)
        .map_err(|_| format!("{} has no tests/ directory", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(".test.js")))
        .collect();
    all_tests.sort();
    if all_tests.is_empty() {
        return Err(format!("no *.test.js files in {}", tests_dir.display()));
    }
    let test_files: Vec<PathBuf> = match filter {
        Some(f) => all_tests
            .into_iter()
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.contains(f)))
            .collect(),
        None => all_tests,
    };
    if test_files.is_empty() {
        return Err(format!("no test file name matches --filter '{}'", filter.unwrap_or("")));
    }

    let out = std::env::temp_dir().join(format!("mist-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&out);
    if !run_build_opt(&src, &out, false, true) {
        return Err("build failed".to_string());
    }
    let runner = out.join(".mist-test-runner.js");
    fs::write(&runner, include_str!("../runtime/mist-test.js")).map_err(|e| e.to_string())?;

    let mut failed = 0;
    for test in &test_files {
        let (ok, stdout, stderr) = match run_node_test(&runner, test, &out, timeout_secs) {
            Ok(r) => r,
            Err(e) => {
                let _ = fs::remove_dir_all(&out);
                return Err(e);
            }
        };
        let name = test.strip_prefix(dir).unwrap_or(test).display();
        if ok {
            println!("PASS {}", name);
            if !stdout.trim().is_empty() {
                println!("{}", stdout.trim_end());
            }
        } else {
            failed += 1;
            println!("FAIL {}", name);
            if !stdout.trim().is_empty() {
                println!("{}", stdout.trim_end());
            }
            if !stderr.trim().is_empty() {
                eprintln!("{}", stderr.trim_end());
            }
        }
    }
    let _ = fs::remove_dir_all(&out);
    println!("{} passed, {} failed", test_files.len() - failed, failed);
    Ok(failed == 0)
}

fn run_build(input: &Path, outdir: &Path, emit_app: bool) -> bool {
    run_build_opt(input, outdir, emit_app, false)
}

fn run_build_opt(input: &Path, outdir: &Path, emit_app: bool, quiet: bool) -> bool {
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
        if !quiet {
            println!("  {}{}", f.out_path, tag);
        }
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
    if !quiet {
        if input.is_dir() && project.app.is_some() {
            report_package_sizes(outdir, &written, project.size_budget);
        }
        println!("compiled {} file(s) → {}/", files.len(), outdir.display());
    }
    true
}

const WECHAT_PACKAGE_LIMIT: u64 = 2 * 1024 * 1024;
const WECHAT_TOTAL_LIMIT: u64 = 20 * 1024 * 1024;

fn human_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.2}MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{}KB", bytes.div_ceil(1024))
    }
}

/// Build-end package-size summary + M1029 warnings against WeChat's upload
/// limits (2MB main, 2MB per subpackage, 20MB total) and the opt-in
/// `config.sizeBudget`. Byte counts are of the emitted files — WeChat measures
/// the uploaded package, so treat the numbers as close, not exact.
fn report_package_sizes(outdir: &Path, written: &[String], budget: Option<u64>) {
    let mut main: u64 = 0;
    let mut subs: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    for rel in written {
        let size = fs::metadata(outdir.join(rel)).map(|m| m.len()).unwrap_or(0);
        match rel.strip_prefix("packages/").and_then(|r| r.split('/').next()) {
            Some(pkg) => *subs.entry(pkg.to_string()).or_insert(0) += size,
            None => main += size,
        }
    }
    let total: u64 = main + subs.values().sum::<u64>();
    let mut parts = vec![format!("main {}", human_size(main))];
    parts.extend(subs.iter().map(|(pkg, s)| format!("{} {}", pkg, human_size(*s))));
    parts.push(format!("total {}", human_size(total)));
    println!("  size: {}", parts.join(", "));

    let warn = |label: &str, size: u64, limit: u64, what: &str| {
        if size > limit {
            eprintln!(
                "warning M1029: {} is {} — exceeds {} ({}); byte counts are approximate, WeChat measures the uploaded package\n  help: split content across src/packages/ subpackages, trim npm/vendor weight, or shrink assets/",
                label,
                human_size(size),
                what,
                human_size(limit)
            );
        }
    };
    let (pkg_limit, pkg_what) = match budget {
        Some(b) if b < WECHAT_PACKAGE_LIMIT => (b, "config.sizeBudget"),
        _ => (WECHAT_PACKAGE_LIMIT, "WeChat's per-package limit"),
    };
    warn("main package", main, pkg_limit, pkg_what);
    for (pkg, size) in &subs {
        warn(&format!("subpackage '{}'", pkg), *size, pkg_limit, pkg_what);
    }
    warn("total output", total, WECHAT_TOTAL_LIMIT, "WeChat's total limit");
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
    let mut param_entries: Vec<String> = files
        .iter()
        .filter(|f| f.is_page)
        .filter_map(|f| {
            f.route_param.as_ref().map(|p| {
                format!("    \"/{}\": {{ {}: string | number }};", f.out_path, p)
            })
        })
        .collect();
    param_entries.sort();
    param_entries.dedup();
    let params_block = if param_entries.is_empty() {
        "  export interface RouteParams {}\n".to_string()
    } else {
        format!("  export interface RouteParams {{\n{}\n  }}\n", param_entries.join("\n"))
    };
    let nav_args = "...params: R extends keyof RouteParams ? [params: RouteParams[R] & Record<string, string | number | boolean>] : [params?: Record<string, string | number | boolean>]";
    let content = format!(
        "// generated by mistc — do not edit\ndeclare module 'mist' {{\n  export type Route = {};\n{}  export function navigate<R extends Route>(route: R, {}): void;\n  export namespace navigate {{\n    function replace<R extends Route>(route: R, {}): void;\n    function back(delta?: number): void;\n    function switchTab(route: Route): void;\n  }}\n}}\n",
        route_union, params_block, nav_args, nav_args
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
    fs::create_dir_all(root.join("tests")).map_err(|e| e.to_string())?;
    let test = "const assert = require('node:assert');\n\nmodule.exports = async () => {\n  const app = bootPage('index');\n  assert.equal(app.data().open.length, 2);\n\n  app.page.toggle(1);\n  await flush();\n  assert.equal(app.data().open.length, 1);\n  assert.ok(app.lastPatch().size < 300, `toggle patch too large: ${app.lastPatch().size} bytes`);\n\n  app.page.add();\n  await flush();\n  assert.equal(app.data().todos.length, 3);\n};\n";
    fs::write(root.join("tests").join("index.test.js"), test).map_err(|e| e.to_string())?;
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
    println!("  tests/index.test.js");
    println!("  project.config.json");
    println!("  mist.d.ts + tsconfig.json   (editor types for 'mist')");
    println!();
    println!("next:");
    println!("  cd {} && mistc build src --watch", name);
    println!("  mistc test                   # run tests/*.test.js in a Node harness");
    println!("  mistc test --snapshots       # pin emitted output against snapshots/");
    println!("  npm install                  # optional: wx.* types for your editor");
    println!("  WeChat DevTools → Import Project → select {}/", name);
    Ok(())
}
