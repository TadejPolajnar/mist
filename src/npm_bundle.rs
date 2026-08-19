//! Bundles bare npm imports (SPEC §12, boundary-rule design) into
//! self-contained CJS vendor files via esbuild, installed once into the same
//! npm cache Tailwind uses. Resolution runs against the project's own
//! node_modules through NODE_PATH; output is memoized per package version.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn vendor_stem(pkg: &str) -> String {
    pkg.trim_start_matches('@').replace('/', "__")
}

pub fn package_root(pkg: &str) -> String {
    let mut parts = pkg.split('/');
    let first = parts.next().unwrap_or(pkg);
    if first.starts_with('@') {
        match parts.next() {
            Some(second) => format!("{}/{}", first, second),
            None => first.to_string(),
        }
    } else {
        first.to_string()
    }
}

pub fn valid_package_name(pkg: &str) -> bool {
    if pkg.is_empty() || pkg.starts_with('/') || pkg.starts_with('.') || pkg.contains("..") {
        return false;
    }
    if pkg.split('/').any(|seg| seg.is_empty()) {
        return false;
    }
    pkg.chars().all(|c| c.is_alphanumeric() || matches!(c, '@' | '/' | '.' | '_' | '-'))
}

fn esbuild_cache_dir() -> PathBuf {
    match std::env::var_os("HOME") {
        Some(home) if !home.is_empty() => {
            PathBuf::from(home).join(".cache").join("mistc").join("esbuild")
        }
        _ => std::env::temp_dir().join("mistc-esbuild"),
    }
}

fn ensure_esbuild() -> Result<PathBuf, String> {
    let dir = esbuild_cache_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let bin = dir.join("node_modules").join(".bin").join("esbuild");
    if !bin.exists() {
        eprintln!("installing esbuild (first npm-import build, needs network)…");
        fs::write(dir.join("package.json"), "{ \"name\": \"mistc-esbuild\", \"private\": true }")
            .map_err(|e| e.to_string())?;
        let out = Command::new("npm")
            .args(["install", "--no-audit", "--no-fund", "esbuild@^0.24"])
            .current_dir(&dir)
            .output()
            .map_err(|e| format!("npm not available: {}", e))?;
        if !out.status.success() {
            return Err(format!("npm install esbuild failed: {}", String::from_utf8_lossy(&out.stderr)));
        }
    }
    Ok(bin)
}

fn package_version(project_root: &Path, pkg: &str) -> Option<String> {
    let manifest = project_root.join("node_modules").join(package_root(pkg)).join("package.json");
    let text = fs::read_to_string(manifest).ok()?;
    let idx = text.find("\"version\"")?;
    let rest = &text[idx + 9..];
    let open = rest.find('"')? + 1;
    let close = rest[open..].find('"')? + open;
    Some(rest[open..close].to_string())
}

pub fn bundle_package(project_root: &Path, pkg: &str) -> Result<String, String> {
    let root_pkg = package_root(pkg);
    if !project_root.join("node_modules").join(&root_pkg).exists() {
        return Err(format!(
            "npm package '{}' not found in {} — run `npm install {}` there first",
            root_pkg,
            project_root.join("node_modules").display(),
            root_pkg
        ));
    }
    let memo = package_version(project_root, pkg).map(|v| {
        esbuild_cache_dir().join(format!("bundle-{}-{}.js", vendor_stem(pkg), v))
    });
    if let Some(memo_path) = &memo {
        if let Ok(cached) = fs::read_to_string(memo_path) {
            return Ok(cached);
        }
    }
    let bin = ensure_esbuild()?;
    let io = std::env::temp_dir().join(format!(
        "mistc-npm-{}-{}",
        std::process::id(),
        vendor_stem(pkg)
    ));
    fs::create_dir_all(&io).map_err(|e| e.to_string())?;
    let stub = io.join("stub.js");
    fs::write(&stub, format!("module.exports = require('{}');\n", pkg)).map_err(|e| e.to_string())?;
    let outfile = io.join("out.js");
    let result = Command::new(&bin)
        .arg(&stub)
        .args(["--bundle", "--format=cjs", "--platform=browser", "--target=es2018", "--log-level=warning"])
        .arg(format!("--outfile={}", outfile.display()))
        .env("NODE_PATH", project_root.join("node_modules"))
        .output()
        .map_err(|e| format!("cannot run esbuild: {}", e))?;
    if !result.status.success() {
        let _ = fs::remove_dir_all(&io);
        return Err(format!(
            "bundling npm package '{}' failed: {}",
            pkg,
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    let js = fs::read_to_string(&outfile).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&io);
    if let Some(memo_path) = &memo {
        let _ = fs::write(memo_path, &js);
    }
    Ok(js)
}
