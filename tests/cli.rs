use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_mistc")
}

#[test]
fn version_and_help() {
    let v = Command::new(bin()).arg("--version").output().unwrap();
    assert!(v.status.success());
    assert!(String::from_utf8_lossy(&v.stdout)
        .contains(&format!("mistc {}", env!("CARGO_PKG_VERSION"))));
    let h = Command::new(bin()).arg("--help").output().unwrap();
    assert!(h.status.success());
    let out = String::from_utf8_lossy(&h.stdout);
    assert!(out.contains("build"), "help: {}", out);
    assert!(out.contains("init"), "help: {}", out);
}

#[test]
fn unknown_command_fails_with_suggestion() {
    let out = Command::new(bin()).arg("bulid").output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("build"));
}

#[test]
fn init_scaffolds_a_compilable_project() {
    let dir = std::env::temp_dir().join("mist-cli-init");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new(bin()).arg("init").arg("demo").current_dir(&dir).output().unwrap();
    assert!(out.status.success(), "init failed: {}", String::from_utf8_lossy(&out.stderr));
    let root = dir.join("demo");
    for f in [
        "src/app.mist",
        "src/pages/index.mist",
        "tests/index.test.js",
        "project.config.json",
        ".gitignore",
        "mist.d.ts",
        "tsconfig.json",
        "package.json",
    ] {
        assert!(root.join(f).exists(), "missing {}", f);
    }
    let project = mistc::compile_project_dir(&root.join("src")).expect("scaffold must compile");
    assert!(project.warnings.is_empty(), "scaffold warnings: {:?}", project.warnings);
    assert!(project.files.iter().any(|f| f.is_page));
}

fn node_available() -> bool {
    Command::new("node").arg("--version").output().is_ok()
}

#[test]
fn test_command_runs_scaffolded_suite() {
    let dir = std::env::temp_dir().join("mist-cli-test-cmd");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new(bin()).arg("init").arg("app").current_dir(&dir).output().unwrap();
    assert!(out.status.success(), "init failed: {}", String::from_utf8_lossy(&out.stderr));
    if !node_available() {
        eprintln!("skipping: node not available");
        return;
    }
    let root = dir.join("app");
    let out = Command::new(bin()).arg("test").current_dir(&root).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "mistc test failed:\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("PASS"), "stdout:\n{}", stdout);
    assert!(stdout.contains("1 passed, 0 failed"), "stdout:\n{}", stdout);

    std::fs::write(
        root.join("tests/broken.test.js"),
        "const assert = require('node:assert');\nmodule.exports = async () => {\n  const app = bootPage('index');\n  assert.equal(app.data().open.length, 99);\n};\n",
    )
    .unwrap();
    let out = Command::new(bin()).arg("test").current_dir(&root).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "failing suite must exit nonzero:\n{}", stdout);
    assert!(stdout.contains("FAIL tests/broken.test.js"), "stdout:\n{}", stdout);
    assert!(stdout.contains("1 passed, 1 failed"), "stdout:\n{}", stdout);

    let out = Command::new(bin())
        .args(["test", "--filter", "index"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(out.status.success(), "filter run failed:\n{}", String::from_utf8_lossy(&out.stdout));

    std::fs::write(
        root.join("tests/broken.test.js"),
        "module.exports = async () => { setInterval(() => {}, 1000); await new Promise(() => {}); };\n",
    )
    .unwrap();
    let out = Command::new(bin())
        .args(["test", "--filter", "broken", "--timeout", "1"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(!out.status.success(), "hung test must fail");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("timed out after 1s"),
        "stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn test_harness_boots_components() {
    let dir = std::env::temp_dir().join("mist-cli-test-component");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new(bin()).arg("init").arg("app").current_dir(&dir).output().unwrap();
    assert!(out.status.success(), "init failed: {}", String::from_utf8_lossy(&out.stderr));
    if !node_available() {
        eprintln!("skipping: node not available");
        return;
    }
    let root = dir.join("app");

    std::fs::create_dir_all(root.join("src/components")).unwrap();
    std::fs::write(
        root.join("src/components/Badge.mist"),
        "---\nimport { state, props } from 'mist'\nconst { label, onPick } = props({ label: '' })\nconst hits = state(0)\nfunction bump() { hits.value++; onPick(hits.value) }\n---\n<span class=\"b\" onTap={bump}>{label} {hits.value}</span>\n",
    )
    .unwrap();

    let index = std::fs::read_to_string(root.join("src/pages/index.mist")).unwrap();
    let index = index.replacen(
        "import { state, derived } from 'mist'\n",
        "import { state, derived } from 'mist'\nimport Badge from '../components/Badge.mist'\n",
        1,
    );
    let index = format!("{}\n<Badge label=\"x\" onPick={{pick}} />\n", index.trim_end());
    let index = index.replacen(
        "function add() {",
        "function pick() {}\n\nfunction add() {",
        1,
    );
    std::fs::write(root.join("src/pages/index.mist"), index).unwrap();

    std::fs::write(
        root.join("tests/badge.test.js"),
        r#"const assert = require('node:assert');
module.exports = async () => {
  const c = bootComponent('badge', { props: { label: 'hi' } });
  assert.equal(c.data().label, 'hi');
  c.comp.bump();
  await flush();
  assert.equal(c.data().hits, 1);
  assert.equal(c.events[0].name, 'pick');
  assert.deepEqual(c.events[0].detail.args, [0]);
  c.setProp('label', 'yo');
  await flush();
  assert.equal(c.data().label, 'yo');
  let msg = '';
  try { bootPage('components/badge/badge'); } catch (e) { msg = e.message; }
  assert.ok(msg.includes('use bootComponent'), msg);
};
"#,
    )
    .unwrap();

    let out = Command::new(bin())
        .args(["test", "--filter", "badge"])
        .current_dir(&root)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "mistc test failed:\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("PASS tests/badge.test.js"), "stdout:\n{}", stdout);
}

#[test]
fn test_command_watch_reruns_on_change() {
    use std::io::BufRead;
    let dir = std::env::temp_dir().join("mist-cli-test-watch");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new(bin()).arg("init").arg("app").current_dir(&dir).output().unwrap();
    assert!(out.status.success(), "init failed: {}", String::from_utf8_lossy(&out.stderr));
    if !node_available() {
        eprintln!("skipping: node not available");
        return;
    }
    let root = dir.join("app");
    let mut child = Command::new(bin())
        .args(["test", "--watch"])
        .current_dir(&root)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout).lines().map_while(|l| l.ok()) {
            if tx.send(line).is_err() {
                return;
            }
        }
    });
    let wait_for = |needle: &str| -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while std::time::Instant::now() < deadline {
            match rx.recv_timeout(std::time::Duration::from_secs(1)) {
                Ok(line) if line.contains(needle) => return true,
                _ => {}
            }
        }
        false
    };
    let first = wait_for("1 passed, 0 failed");
    if !first {
        let _ = child.kill();
        panic!("initial watch run did not report a passing suite");
    }
    if !wait_for("watching") {
        let _ = child.kill();
        panic!("watcher never registered");
    }
    std::fs::write(
        root.join("tests/second.test.js"),
        "module.exports = async () => { bootPage('index'); };\n",
    )
    .unwrap();
    let rerun = wait_for("2 passed, 0 failed");
    let _ = child.kill();
    let _ = child.wait();
    assert!(rerun, "watch did not rerun after a test file was added");
}

#[test]
fn test_command_requires_project_layout() {
    let dir = std::env::temp_dir().join("mist-cli-test-cmd-empty");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new(bin()).arg("test").current_dir(&dir).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("src/app.mist"));
}

#[test]
fn init_refuses_existing_directory() {
    let dir = std::env::temp_dir().join("mist-cli-init-exists");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("taken")).unwrap();
    let out = Command::new(bin()).arg("init").arg("taken").current_dir(&dir).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("already exists"));
}

#[test]
fn stale_outputs_are_pruned_on_rebuild() {
    let dir = std::env::temp_dir().join("mist-cli-prune");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src/pages")).unwrap();
    std::fs::write(
        dir.join("src/app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let page = "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span class=\"mine\">{n.value}</span>\n<style>.mine { color: red; }</style>\n";
    std::fs::write(dir.join("src/pages/index.mist"), page).unwrap();
    std::fs::write(dir.join("src/pages/old.mist"), page).unwrap();
    let out_dir = dir.join("dist");
    let run = |args: &[&str]| {
        let out = Command::new(bin()).args(args).current_dir(&dir).output().unwrap();
        assert!(out.status.success(), "build failed: {}", String::from_utf8_lossy(&out.stderr));
    };
    run(&["build", "src", "-o", "dist"]);
    assert!(out_dir.join("pages/old/old.js").exists());
    assert!(out_dir.join("pages/index/index.wxss").exists());

    std::fs::remove_file(dir.join("src/pages/old.mist")).unwrap();
    let no_style = "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n";
    std::fs::write(dir.join("src/pages/index.mist"), no_style).unwrap();
    run(&["build", "src", "-o", "dist"]);
    assert!(!out_dir.join("pages/old/old.js").exists(), "removed page must be pruned");
    assert!(!out_dir.join("pages/old").exists(), "empty page dir must be pruned");
    assert!(!out_dir.join("pages/index/index.wxss").exists(), "stale wxss must be pruned");
    assert!(out_dir.join("pages/index/index.js").exists(), "live files stay");
}

#[test]
fn assets_are_copied_and_pruned() {
    let dir = std::env::temp_dir().join("mist-asset-copy");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src/pages")).unwrap();
    std::fs::write(
        dir.join("src/app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    let out_dir = dir.join("dist");
    let run = |args: &[&str]| {
        let out = Command::new(bin()).args(args).current_dir(&dir).output().unwrap();
        assert!(out.status.success(), "build failed: {}", String::from_utf8_lossy(&out.stderr));
    };

    std::fs::create_dir_all(dir.join("src/assets/icons")).unwrap();
    let bytes: &[u8] = &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0xde, 0xad, 0xbe, 0xef];
    std::fs::write(dir.join("src/assets/icons/home.png"), bytes).unwrap();
    run(&["build", "src", "-o", "dist"]);
    let copied = out_dir.join("assets/icons/home.png");
    assert!(copied.exists(), "asset must be copied");
    assert_eq!(std::fs::read(&copied).unwrap(), bytes, "asset must be byte-identical");

    std::fs::remove_file(dir.join("src/assets/icons/home.png")).unwrap();
    run(&["build", "src", "-o", "dist"]);
    assert!(!copied.exists(), "removed asset must be pruned");

    std::fs::remove_dir_all(dir.join("src/assets")).unwrap();
    run(&["build", "src", "-o", "dist"]);
    assert!(!out_dir.join("assets").exists(), "no assets dir when source has none");

    std::fs::create_dir_all(dir.join("src/assets")).unwrap();
    run(&["build", "src", "-o", "dist"]);
}

#[test]
fn user_sitemap_json_overrides_default() {
    let dir = std::env::temp_dir().join("mist-sitemap-override");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src/pages")).unwrap();
    std::fs::write(
        dir.join("src/app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    let out_dir = dir.join("dist");
    let run = |args: &[&str]| {
        let out = Command::new(bin()).args(args).current_dir(&dir).output().unwrap();
        assert!(out.status.success(), "build failed: {}", String::from_utf8_lossy(&out.stderr));
    };

    std::fs::write(
        dir.join("src/sitemap.json"),
        "{ \"rules\": [ { \"action\": \"allow\", \"page\": \"mist-sitemap-override-marker\" } ] }\n",
    )
    .unwrap();
    run(&["build", "src", "-o", "dist"]);
    let sitemap = std::fs::read_to_string(out_dir.join("sitemap.json")).unwrap();
    assert!(sitemap.contains("mist-sitemap-override-marker"), "sitemap: {}", sitemap);

    std::fs::remove_file(dir.join("src/sitemap.json")).unwrap();
    run(&["build", "src", "-o", "dist"]);
    let sitemap = std::fs::read_to_string(out_dir.join("sitemap.json")).unwrap();
    assert!(sitemap.contains("\"rules\": []"), "sitemap: {}", sitemap);
}

#[test]
fn theme_json_and_workers_are_copied_and_pruned() {
    let dir = std::env::temp_dir().join("mist-theme-workers");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src/pages")).unwrap();
    std::fs::write(
        dir.join("src/app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    let out_dir = dir.join("dist");
    let run = |args: &[&str]| {
        let out = Command::new(bin()).args(args).current_dir(&dir).output().unwrap();
        assert!(out.status.success(), "build failed: {}", String::from_utf8_lossy(&out.stderr));
    };

    // both absent → no-op, no error
    run(&["build", "src", "-o", "dist"]);
    assert!(!out_dir.join("theme.json").exists());
    assert!(!out_dir.join("workers").exists());

    std::fs::write(
        dir.join("src/theme.json"),
        "{ \"light\": { \"navBgColor\": \"#fff\" } }\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src/workers/nested")).unwrap();
    std::fs::write(dir.join("src/workers/index.js"), "// worker entry\n").unwrap();
    std::fs::write(dir.join("src/workers/nested/util.js"), "// nested util\n").unwrap();
    run(&["build", "src", "-o", "dist"]);
    assert!(out_dir.join("theme.json").exists(), "theme.json must be copied");
    let theme = std::fs::read_to_string(out_dir.join("theme.json")).unwrap();
    assert!(theme.contains("navBgColor"), "theme: {}", theme);
    assert!(out_dir.join("workers/index.js").exists(), "worker entry must be copied");
    assert!(out_dir.join("workers/nested/util.js").exists(), "nested worker file must be copied");

    std::fs::remove_file(dir.join("src/theme.json")).unwrap();
    std::fs::remove_file(dir.join("src/workers/nested/util.js")).unwrap();
    std::fs::remove_dir(dir.join("src/workers/nested")).unwrap();
    run(&["build", "src", "-o", "dist"]);
    assert!(!out_dir.join("theme.json").exists(), "removed theme.json must be pruned");
    assert!(!out_dir.join("workers/nested/util.js").exists(), "removed worker file must be pruned");
    assert!(!out_dir.join("workers/nested").exists(), "empty worker subdir must be pruned");
    assert!(out_dir.join("workers/index.js").exists(), "live worker file stays");

    std::fs::remove_dir_all(dir.join("src/workers")).unwrap();
    run(&["build", "src", "-o", "dist"]);
    assert!(!out_dir.join("workers").exists(), "no workers dir when source has none");
}

#[test]
fn routes_dts_written_next_to_existing_mist_dts() {
    let dir = std::env::temp_dir().join("mist-routes-dts-written");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src/pages")).unwrap();
    std::fs::create_dir_all(dir.join("src/packages/shop/pages")).unwrap();
    std::fs::write(
        dir.join("src/app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/packages/shop/pages/cart.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src/pages/item")).unwrap();
    std::fs::write(
        dir.join("src/pages/item/[id].mist"),
        "---\nimport {{ state }} from 'mist'\nconst id = state('')\n---\n<span>{{id.value}}</span>\n".replace("{{", "{").replace("}}", "}"),
    )
    .unwrap();
    std::fs::write(dir.join("mist.d.ts"), "declare module 'mist' {}\n").unwrap();
    let out_dir = dir.join("dist");
    let out = Command::new(bin()).args(["build", "src", "-o", "dist"]).current_dir(&dir).output().unwrap();
    assert!(out.status.success(), "build failed: {}", String::from_utf8_lossy(&out.stderr));

    let routes_dts_path = dir.join("mist-routes.d.ts");
    assert!(routes_dts_path.exists(), "mist-routes.d.ts must be written next to mist.d.ts");
    let content = std::fs::read_to_string(&routes_dts_path).unwrap();
    assert!(content.contains("declare module 'mist'"), "content:\n{}", content);
    assert!(content.contains("\"/pages/index/index\""), "content:\n{}", content);
    assert!(content.contains("\"/packages/shop/pages/cart/cart\""), "content:\n{}", content);
    assert!(content.contains("export function navigate"), "content:\n{}", content);
    assert!(content.contains("function switchTab"), "content:\n{}", content);
    assert!(content.contains("\"/pages/item/item\""), "content:\n{}", content);
    assert!(
        content.contains("\"/pages/item/item\": { id: string | number };"),
        "content:\n{}",
        content
    );
    assert!(content.contains("keyof RouteParams"), "content:\n{}", content);

    // never in dist, never in the manifest
    assert!(!out_dir.join("mist-routes.d.ts").exists(), "must not be emitted into dist/");
    let manifest = std::fs::read_to_string(out_dir.join(".mist-manifest")).unwrap();
    assert!(!manifest.contains("mist-routes.d.ts"), "manifest:\n{}", manifest);
}

#[test]
fn routes_dts_skipped_without_mist_dts() {
    let dir = std::env::temp_dir().join("mist-routes-dts-skipped");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src/pages")).unwrap();
    std::fs::write(
        dir.join("src/app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    let out = Command::new(bin()).args(["build", "src", "-o", "dist"]).current_dir(&dir).output().unwrap();
    assert!(out.status.success(), "build failed: {}", String::from_utf8_lossy(&out.stderr));
    assert!(!dir.join("mist-routes.d.ts").exists(), "must not be written without mist.d.ts present");
}

#[test]
fn routes_dts_not_rewritten_when_unchanged() {
    let dir = std::env::temp_dir().join("mist-routes-dts-stable");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src/pages")).unwrap();
    std::fs::write(
        dir.join("src/app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(dir.join("mist.d.ts"), "declare module 'mist' {}\n").unwrap();
    let run = || {
        let out = Command::new(bin()).args(["build", "src", "-o", "dist"]).current_dir(&dir).output().unwrap();
        assert!(out.status.success(), "build failed: {}", String::from_utf8_lossy(&out.stderr));
    };
    run();
    let routes_dts_path = dir.join("mist-routes.d.ts");
    let mtime1 = std::fs::metadata(&routes_dts_path).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    run();
    let mtime2 = std::fs::metadata(&routes_dts_path).unwrap().modified().unwrap();
    assert_eq!(mtime1, mtime2, "unchanged route set must not rewrite the file");
}

#[test]
fn npm_import_bundles_and_runs_end_to_end() {
    if !node_available() {
        eprintln!("skipping: node not available");
        return;
    }
    let dir = std::env::temp_dir().join("mist-cli-npm-bundle");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src/pages")).unwrap();
    std::fs::create_dir_all(dir.join("tests")).unwrap();
    std::fs::create_dir_all(dir.join("node_modules/greeting")).unwrap();
    std::fs::write(
        dir.join("node_modules/greeting/package.json"),
        "{ \"name\": \"greeting\", \"version\": \"1.0.1\", \"main\": \"index.js\" }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("node_modules/greeting/index.js"),
        "function greet(name) { return 'hi ' + name; }\ngreet.shout = function (name) { return ('hi ' + name).toUpperCase(); };\nmodule.exports = greet;\nmodule.exports.shout = greet.shout;\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/pages/index.mist"),
        "---\nimport { state } from 'mist'\nimport greet from 'greeting'\nimport { shout } from 'greeting'\nconst msg = state('')\nfunction hello() {\n  msg.value = greet('mist') + '/' + shout('mist')\n}\n---\n<span onTap={hello}>{msg.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("tests/greet.test.js"),
        "const assert = require('node:assert');\nmodule.exports = async () => {\n  const app = bootPage('index');\n  app.page.hello();\n  await flush();\n  assert.equal(app.data().msg, 'hi mist/HI MIST');\n};\n",
    )
    .unwrap();
    let out = Command::new(bin()).args(["build", "src", "-o", "dist"]).current_dir(&dir).output().unwrap();
    assert!(out.status.success(), "build failed: {}", String::from_utf8_lossy(&out.stderr));
    let vendor = std::fs::read_to_string(dir.join("dist/vendor/greeting.js")).expect("vendor bundle missing");
    assert!(vendor.contains("hi "), "vendor:\n{}", vendor);
    let page = std::fs::read_to_string(dir.join("dist/pages/index/index.js")).unwrap();
    assert!(page.contains("require('../../vendor/greeting.js')"), "page:\n{}", page);

    let out = Command::new(bin()).arg("test").current_dir(&dir).output().unwrap();
    assert!(
        out.status.success(),
        "mistc test failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("1 passed, 0 failed"));
}

#[test]
fn snapshot_mode_writes_diffs_and_updates() {
    let dir = std::env::temp_dir().join("mist-cli-snapshots");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new(bin()).arg("init").arg("app").current_dir(&dir).output().unwrap();
    assert!(out.status.success(), "init failed: {}", String::from_utf8_lossy(&out.stderr));
    let root = dir.join("app");

    let out = Command::new(bin()).args(["test", "--snapshots"]).current_dir(&root).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "first run failed:\n{}\n{}", stdout, String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains("written (first run)"), "stdout:\n{}", stdout);
    assert!(root.join("snapshots/app.json").is_file(), "goldens missing");

    let out = Command::new(bin()).args(["test", "--snapshots"]).current_dir(&root).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success() && stdout.contains("snapshot(s) match"), "stdout:\n{}", stdout);

    let page = root.join("src/pages/index.mist");
    let src = std::fs::read_to_string(&page).unwrap();
    assert!(src.contains("Add todo"), "scaffold changed — pick a new drift edit");
    std::fs::write(&page, src.replace("Add todo", "Add task")).unwrap();
    let out = Command::new(bin()).args(["test", "--snapshots"]).current_dir(&root).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "drift must exit nonzero:\n{}", stdout);
    assert!(stdout.contains("CHANGED") && stdout.contains("first difference"), "stdout:\n{}", stdout);

    let out = Command::new(bin()).args(["test", "--update"]).current_dir(&root).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success() && stdout.contains("updated"), "stdout:\n{}", stdout);
    let out = Command::new(bin()).args(["test", "--snapshots"]).current_dir(&root).output().unwrap();
    assert!(out.status.success(), "post-update mismatch:\n{}", String::from_utf8_lossy(&out.stdout));

    let out = Command::new(bin()).args(["test", "--snapshots", "--watch"]).current_dir(&root).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("does not combine"), "stderr must explain");
}

#[test]
fn build_reports_sizes_and_m1029_over_budget() {
    let dir = std::env::temp_dir().join("mist-cli-size-budget");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new(bin()).arg("init").arg("app").current_dir(&dir).output().unwrap();
    assert!(out.status.success(), "init failed: {}", String::from_utf8_lossy(&out.stderr));
    let root = dir.join("app");

    let out = Command::new(bin()).args(["build", "src"]).current_dir(&root).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(stdout.contains("size: main"), "stdout:\n{}", stdout);
    assert!(!String::from_utf8_lossy(&out.stderr).contains("M1029"));

    let app = root.join("src/app.mist");
    let src = std::fs::read_to_string(&app).unwrap();
    std::fs::write(&app, src.replace("export const config = {", "export const config = {\n  sizeBudget: '1KB',")).unwrap();
    let out = Command::new(bin()).args(["build", "src"]).current_dir(&root).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "over-budget is warning-tier, not an error");
    assert!(stderr.contains("M1029") && stderr.contains("config.sizeBudget"), "stderr:\n{}", stderr);
}

#[test]
fn upload_requires_build_output() {
    let dir = std::env::temp_dir().join("mist-cli-upload-no-dist");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new(bin()).arg("init").arg("app").current_dir(&dir).output().unwrap();
    assert!(out.status.success(), "init failed: {}", String::from_utf8_lossy(&out.stderr));
    let root = dir.join("app");

    let out = Command::new(bin()).args(["upload", "--preview"]).current_dir(&root).output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("run mistc build"), "stderr:\n{}", stderr);
}

#[test]
fn upload_rejects_tourist_appid() {
    let dir = std::env::temp_dir().join("mist-cli-upload-tourist");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new(bin()).arg("init").arg("app").current_dir(&dir).output().unwrap();
    assert!(out.status.success(), "init failed: {}", String::from_utf8_lossy(&out.stderr));
    let root = dir.join("app");

    let out = Command::new(bin()).args(["build", "src", "-o", "dist"]).current_dir(&root).output().unwrap();
    assert!(out.status.success(), "build failed: {}", String::from_utf8_lossy(&out.stderr));

    let key = std::env::temp_dir().join("mist-cli-upload-tourist-key.pem");
    std::fs::write(&key, "not a real key").unwrap();

    let out = Command::new(bin())
        .args(["upload", "--preview", "--key"])
        .arg(&key)
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("tourist appid"), "stderr:\n{}", stderr);
}

#[test]
fn upload_requires_key() {
    let dir = std::env::temp_dir().join("mist-cli-upload-no-key");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new(bin()).arg("init").arg("app").current_dir(&dir).output().unwrap();
    assert!(out.status.success(), "init failed: {}", String::from_utf8_lossy(&out.stderr));
    let root = dir.join("app");

    let out = Command::new(bin()).args(["build", "src", "-o", "dist"]).current_dir(&root).output().unwrap();
    assert!(out.status.success(), "build failed: {}", String::from_utf8_lossy(&out.stderr));

    let config_path = root.join("project.config.json");
    let config = std::fs::read_to_string(&config_path).unwrap();
    let config = config.replace("touristappid", "wxabc1234567890def");
    std::fs::write(&config_path, config).unwrap();

    let out = Command::new(bin())
        .args(["upload", "--preview"])
        .current_dir(&root)
        .env_remove("MISTC_UPLOAD_KEY")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--key"), "stderr:\n{}", stderr);
    assert!(stderr.contains("MISTC_UPLOAD_KEY"), "stderr:\n{}", stderr);
}
