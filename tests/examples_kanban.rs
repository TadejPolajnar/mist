use std::fs;
use std::path::Path;
use std::process::Command;

fn build_kanban() -> std::path::PathBuf {
    let out = std::env::temp_dir().join("mist-kanban-gate");
    let _ = fs::remove_dir_all(&out);
    let result = Command::new(env!("CARGO_BIN_EXE_mistc"))
        .args(["build", "examples/kanban/src", "-o"])
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

fn tag_end(s: &str, start: usize) -> usize {
    let bytes = s.as_bytes();
    let mut i = start;
    let mut quote = 0u8;
    while i < bytes.len() {
        let c = bytes[i];
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c == b'>' {
            return i;
        }
        i += 1;
    }
    s.len()
}

#[test]
fn kanban_example_passes_all_gates() {
    let out = build_kanban();

    for (store, envelope) in [
        ("board", "{ persist: 'kanban.board', version: 1 }"),
        ("team", "{ persist: 'kanban.team', version: 1 }"),
        ("prefs", "{ persist: 'kanban.prefs', version: 1 }"),
    ] {
        let js = fs::read_to_string(out.join(format!("stores/{}.js", store))).unwrap();
        assert!(js.contains(envelope), "{} persistence missing:\n{}", store, js);
    }

    let mut js_files = Vec::new();
    all_js(&out, &mut js_files);
    for (p, js) in &js_files {
        assert!(!js.contains(".this."), "corrupted member call in {}:\n{}", p.display(), js);
    }

    let board_js = fs::read_to_string(out.join("pages/board/board.js")).unwrap();
    assert!(board_js.contains("'grouped', 'id'"), "grouped must use keyed diff:\n{}", board_js);
    assert!(board_js.contains("['board', 'prefs', 'team']"), "grouped deps must harvest team via whoOf helper:\n{}", board_js);

    let board_wxml = fs::read_to_string(out.join("pages/board/board.wxml")).unwrap();
    assert!(board_wxml.contains("kanban-card"), "board must render KanbanCard as a real component:\n{}", board_wxml);
    let board_json = fs::read_to_string(out.join("pages/board/board.json")).unwrap();
    assert!(board_json.contains("kanban-card"), "board.json missing usingComponents:\n{}", board_json);

    for page in ["board", "backlog", "card", "team"] {
        let wxml = fs::read_to_string(out.join(format!("pages/{}/{}.wxml", page, page))).unwrap();
        for tag_start in ["<view", "<navigator"] {
            let mut rest = wxml.as_str();
            while let Some(pos) = rest.find(tag_start) {
                let end = tag_end(rest, pos);
                let tag = &rest[pos..end];
                if tag.contains("bindtap") || tag.contains("catchtap") {
                    assert!(
                        tag.contains("hover-class=\"pressed\""),
                        "tappable tag without hover-class in pages/{}:\n{}",
                        page,
                        tag
                    );
                }
                rest = &rest[end..];
            }
        }
    }

    let card_wxml = fs::read_to_string(out.join("components/kanban-card/kanban-card.wxml")).unwrap();
    let mut rest = card_wxml.as_str();
    while let Some(pos) = rest.find("<view") {
        let end = tag_end(rest, pos);
        let tag = &rest[pos..end];
        if tag.contains("bindtap") || tag.contains("catchtap") {
            assert!(
                tag.contains("hover-class=\"pressed\""),
                "kanban-card tappable tag without hover-class:\n{}",
                tag
            );
        }
        rest = &rest[end..];
    }

    let app_wxss = fs::read_to_string(out.join("app.wxss")).unwrap();
    assert!(app_wxss.contains(".pressed"), "app.wxss missing .pressed:\n{}", app_wxss);

    for pure in ["section-header", "tag", "empty-state", "cell"] {
        let wxss = out.join(format!("components/{}/{}.wxss", pure, pure));
        assert!(!wxss.exists(), "pure component emitted wxss: {}", wxss.display());
    }
    for real in ["kanban-card", "stepper"] {
        assert!(
            out.join(format!("components/{}/{}.js", real, real)).exists(),
            "{} must stay a real component",
            real
        );
    }

    let boot = format!(
        r#"
global.App = () => {{}};
global.Page = (o) => {{ global.__page = o; }};
global.Component = () => {{}};
global.wx = {{
  getStorageSync: () => undefined,
  setStorageSync: () => {{}},
  onAppHide: () => {{}},
  navigateTo: () => {{}},
}};
require('{}');
const page = global.__page;
page.setData = function (patch) {{ Object.assign(this.data, patch); }};
page.onLoad({{}});
setTimeout(() => {{
  const g = page.data.grouped;
  if (g.map(c => c.id).join(',') !== 'todo,doing,review,done') throw new Error('columns wrong: ' + g.map(c => c.id));
  if (g.map(c => c.count).join(',') !== '3,2,2,2') throw new Error('counts wrong: ' + g.map(c => c.count));
  if (g[0].cards.map(c => c.id).join(',') !== '1,2,3') throw new Error('todo order wrong');
  if (g[0].cards[0].first !== true || g[0].cards[2].last !== true) throw new Error('first/last flags wrong');
  if (g[1].cards[1].who !== '🐱 小美') throw new Error('assignee lookup wrong: ' + g[1].cards[1].who);
  page.down(1);
  setTimeout(() => {{
    if (page.data.grouped[0].cards.map(c => c.id).join(',') !== '2,1,3') throw new Error('reorder failed');
    page.right(1);
    page.right(2);
    setTimeout(() => {{
      const g2 = page.data.grouped;
      if (g2[1].count !== 4) throw new Error('cross-column move failed: ' + g2[1].count);
      if (g2[1].over !== true) throw new Error('wip overflow flag must trip at 4 > 3');
      console.log('BOARD OK');
    }}, 10);
  }}, 10);
}}, 10);
"#,
        out.join("pages/board/board.js").display().to_string().replace('\\', "/")
    );
    let node = Command::new("node").arg("-e").arg(&boot).output();
    match node {
        Ok(o) => {
            assert!(
                o.status.success() && String::from_utf8_lossy(&o.stdout).contains("BOARD OK"),
                "board.js boot failed:\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
        }
        Err(_) => eprintln!("skipping node boot: node not available"),
    }

    let card_boot = format!(
        r#"
global.App = () => {{}};
global.Page = (o) => {{ global.__page = o; }};
global.Component = () => {{}};
global.wx = {{
  getStorageSync: () => undefined,
  setStorageSync: () => {{}},
  onAppHide: () => {{}},
  navigateBack: () => {{}},
}};
require('{}');
const page = global.__page;
page.setData = function (patch) {{ Object.assign(this.data, patch); }};
page.onLoad({{ id: '4' }});
if (page.data.missing) throw new Error('missing flag must stay false before first flush');
setTimeout(() => {{
  if (page.data.missing) throw new Error('card 4 must not be missing');
  if (page.data.card.colName !== '进行中') throw new Error('colName wrong: ' + page.data.card.colName);
  const bo = page.data.people.find(m => m.id === 'bo');
  if (!bo || bo.picked !== true) throw new Error('assignee picked flag wrong');
  page.pick('yan');
  setTimeout(() => {{
    const yan = page.data.people.find(m => m.id === 'yan');
    if (!yan || yan.picked !== true) throw new Error('reassign failed');
    console.log('CARD OK');
  }}, 10);
}}, 10);
"#,
        out.join("pages/card/card.js").display().to_string().replace('\\', "/")
    );
    let card_node = Command::new("node").arg("-e").arg(&card_boot).output();
    match card_node {
        Ok(o) => {
            assert!(
                o.status.success() && String::from_utf8_lossy(&o.stdout).contains("CARD OK"),
                "card.js boot failed:\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
        }
        Err(_) => eprintln!("skipping node boot: node not available"),
    }
}
