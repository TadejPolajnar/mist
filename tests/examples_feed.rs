use std::fs;
use std::path::Path;
use std::process::Command;

fn build_feed() -> std::path::PathBuf {
    let out = std::env::temp_dir().join("mist-feed-gate");
    let _ = fs::remove_dir_all(&out);
    let result = Command::new(env!("CARGO_BIN_EXE_mistc"))
        .args(["build", "examples/feed/src", "-o"])
        .arg(&out)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(result.status.success(), "build failed: {}", String::from_utf8_lossy(&result.stderr));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(!stderr.contains("warning"), "build must be warning-free:\n{}", stderr);
    out
}

fn all_js(dir: &Path, acc: &mut Vec<(std::path::PathBuf, String)>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let p = entry.path();
        if p.is_dir() {
            all_js(&p, acc);
        } else if p.extension().and_then(|e| e.to_str()) == Some("js") {
            acc.push((p.clone(), fs::read_to_string(&p).unwrap()));
        }
    }
}

#[test]
fn feed_example_passes_all_gates() {
    let out = build_feed();

    let mut js_files = Vec::new();
    all_js(&out, &mut js_files);
    for (p, js) in &js_files {
        assert!(!js.contains(".this."), "corrupted member call in {}:\n{}", p.display(), js);
    }

    let feed_js = fs::read_to_string(out.join("pages/feed/feed.js")).unwrap();
    assert!(
        !feed_js.contains("posts: "),
        "posts must be dead-data eliminated, never seeded into data:\n{}",
        feed_js
    );
    assert!(
        feed_js.contains("this._posts = this.generate(0)"),
        "unbound state init must rewrite frontmatter calls to methods:\n{}",
        feed_js
    );

    let boot = format!(
        r#"
global.App = () => {{}};
global.Page = (o) => {{ global.__page = o; }};
global.Component = () => {{}};
global.wx = {{ getStorageSync: () => undefined, setStorageSync: () => {{}}, onAppHide: () => {{}}, switchTab: () => {{}} }};
require('{feed}');
const page = global.__page;
let last = null; let rejected = 0;
page.setData = function (patch) {{
  const size = Buffer.byteLength(JSON.stringify(patch));
  if (size > 1024 * 1024) {{ rejected++; throw new Error('too big: ' + size); }}
  last = {{ keys: Object.keys(patch), size }};
  Object.assign(this.data, patch);
}};
page.onLoad({{}});
setTimeout(() => {{
  if ('posts' in page.data) throw new Error('posts leaked into data');
  if (page._posts.length !== 1000) throw new Error('unbound posts wrong: ' + page._posts.length);
  if (page.data.visible.length !== 50) throw new Error('initial page wrong: ' + page.data.visible.length);
  if (last.size > 100000) throw new Error('seed setData too large: ' + last.size);
  const before = page.data.visible[2].likes;
  page.toggleLike(page.data.visible[2].id);
  setTimeout(() => {{
    if (last.size > 200) throw new Error('like patch not path-precise: ' + last.size);
    if (page.data.visible[2].likes !== before + 1) throw new Error('like not applied');
    const lab = require('{lab}');
    lab.setFullRender(true);
    setTimeout(() => {{
      if (rejected < 1) throw new Error('oversized setData was not rejected');
      if (page.data.visible.length !== 50) throw new Error('rollback failed: ' + page.data.visible.length);
      if (rejected > 3) throw new Error('rejection retry not damped: ' + rejected);
      lab.setFullRender(false);
      setTimeout(() => {{
        page.toggleLike(page.data.visible[3].id);
        setTimeout(() => {{
          if (page.data.visible.length !== 50) throw new Error('recovery failed');
          if (last.size > 200) throw new Error('post-recovery like not path-precise: ' + last.size);
          console.log('FEED OK');
        }}, 30);
      }}, 50);
    }}, 50);
  }}, 30);
}}, 30);
"#,
        feed = out.join("pages/feed/feed.js").display().to_string().replace('\\', "/"),
        lab = out.join("stores/lab.js").display().to_string().replace('\\', "/")
    );
    let node = Command::new("node").arg("-e").arg(&boot).output();
    match node {
        Ok(o) => {
            assert!(
                o.status.success() && String::from_utf8_lossy(&o.stdout).contains("FEED OK"),
                "feed.js boot failed:\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
        }
        Err(_) => eprintln!("skipping node boot: node not available"),
    }
}
