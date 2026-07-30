use std::fs;
use std::path::PathBuf;
use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage = "usage: mistc build <src-dir | entry.mist> [-o <outdir>] [--app]";
    if args.len() < 3 || args[1] != "build" {
        eprintln!("{}", usage);
        exit(1);
    }
    let input = PathBuf::from(&args[2]);
    let outdir = match args.iter().position(|a| a == "-o") {
        Some(i) if i + 1 < args.len() => PathBuf::from(&args[i + 1]),
        _ => PathBuf::from("dist"),
    };
    let emit_app = args.iter().any(|a| a == "--app");

    let project = if input.is_dir() {
        mistc::compile_project_dir(&input)
    } else {
        mistc::compile_project(&input)
    };
    let project = match project {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            exit(1);
        }
    };
    let files = &project.files;

    fs::create_dir_all(&outdir).expect("cannot create output dir");
    if !project.tailwind_css.is_empty() {
        fs::write(outdir.join("tw-shared.wxss"), &project.tailwind_css).unwrap();
        fs::write(outdir.join("tw-theme.wxss"), &project.tailwind_theme_css).unwrap();
    }
    for c in &project.unknown_classes {
        eprintln!("warning M1002: unknown class '{}' (no WXSS generated)", c);
    }
    for s in &project.dropped_selectors {
        eprintln!("warning M1006: dropped rule — WXSS cannot express selector '{}'", s.trim());
    }
    for f in files {
        let base = outdir.join(&f.out_path);
        if let Some(parent) = base.parent() {
            fs::create_dir_all(parent).expect("cannot create output dir");
        }
        if !f.output.wxml.is_empty() {
            fs::write(base.with_extension("wxml"), &f.output.wxml).unwrap();
        }
        if !f.output.js.is_empty() {
            fs::write(base.with_extension("js"), &f.output.js).unwrap();
        }
        if !f.output.wxss.is_empty() {
            fs::write(base.with_extension("wxss"), &f.output.wxss).unwrap();
        }
        if let Some(json) = &f.output.json {
            fs::write(base.with_extension("json"), json).unwrap();
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
    fs::write(outdir.join("mist-rt.js"), mistc::RUNTIME).unwrap();

    if let Some(app) = &project.app {
        fs::write(outdir.join("app.js"), &app.js).unwrap();
        fs::write(outdir.join("app.json"), &app.json).unwrap();
        fs::write(outdir.join("app.wxss"), &app.wxss).unwrap();
        fs::write(outdir.join("sitemap.json"), "{ \"rules\": [] }\n").unwrap();
    } else if emit_app {
        let page = files
            .iter()
            .find(|f| f.is_page)
            .map(|f| f.out_path.clone())
            .unwrap_or_else(|| "index".to_string());
        fs::write(
            outdir.join("app.js"),
            "const rt = require('./mist-rt.js');\nrt.observePerf();\nApp({ __perf: rt.perfEntries });\n",
        )
        .unwrap();
        fs::write(
            outdir.join("app.json"),
            format!("{{ \"pages\": [\"{}\"], \"sitemapLocation\": \"sitemap.json\" }}\n", page),
        )
        .unwrap();
        fs::write(outdir.join("sitemap.json"), "{ \"rules\": [] }\n").unwrap();
        fs::write(outdir.join("app.wxss"), "").unwrap();
        fs::write(
            outdir.join("project.config.json"),
            "{ \"appid\": \"touristappid\", \"compileType\": \"miniprogram\", \"setting\": { \"es6\": true } }\n",
        )
        .unwrap();
    }
    println!("compiled {} file(s) → {}/", files.len(), outdir.display());
}
