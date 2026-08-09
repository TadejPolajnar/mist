use std::fs;
use std::path::Path;
use std::process::Command;

fn build_portfolio() -> std::path::PathBuf {
    let out = std::env::temp_dir().join("mist-portfolio-gate");
    let _ = fs::remove_dir_all(&out);
    let result = Command::new(env!("CARGO_BIN_EXE_mistc"))
        .args(["build", "examples/portfolio/src", "-o"])
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

fn derive_index(js: &str, name: &str) -> usize {
    js.find(&format!("__o, '{}'", name))
        .unwrap_or_else(|| panic!("derived '{}' missing:\n{}", name, js))
}

#[test]
fn portfolio_example_passes_all_gates() {
    let out = build_portfolio();

    let market_js = fs::read_to_string(out.join("stores/market.js")).unwrap();
    assert!(market_js.contains("{ persist: 'folio.market', version: 1 }"), "market persistence missing:\n{}", market_js);
    let prefs_js = fs::read_to_string(out.join("stores/prefs.js")).unwrap();
    assert!(prefs_js.contains("{ persist: 'folio.prefs', version: 1 }"), "prefs persistence missing:\n{}", prefs_js);

    let mut js_files = Vec::new();
    all_js(&out, &mut js_files);
    for (p, js) in &js_files {
        assert!(!js.contains(".this."), "corrupted member call in {}:\n{}", p.display(), js);
    }

    let dash_js = fs::read_to_string(out.join("pages/dashboard/dashboard.js")).unwrap();
    assert_eq!(dash_js.matches("rt.derive(").count(), 13, "dashboard must hold the full 13-derived DAG:\n{}", dash_js);

    for (upstream, downstream) in [
        ("enriched", "totalValue"),
        ("enriched", "totalCost"),
        ("totalValue", "totalPnl"),
        ("totalCost", "totalPnl"),
        ("totalPnl", "totalPnlBp"),
        ("totalPnlBp", "headline"),
        ("enriched", "sectors"),
        ("sectors", "allocation"),
        ("totalValue", "allocation"),
        ("alerts", "alertCount"),
    ] {
        assert!(
            derive_index(&dash_js, upstream) < derive_index(&dash_js, downstream),
            "'{}' must derive before '{}':\n{}",
            upstream,
            downstream,
            dash_js
        );
    }

    assert!(dash_js.contains("'totalPnl', null, () => this.data.totalValue - this.data.totalCost, ['totalCost', 'totalValue']"), "diamond deps missing:\n{}", dash_js);
    assert!(dash_js.contains("['market', 'prefs']"), "alerts skip-link deps missing:\n{}", dash_js);
    assert!(dash_js.contains("'allocation', 'name'"), "allocation must use keyed diff:\n{}", dash_js);
    assert!(dash_js.contains("'movers', 'id'"), "movers must use keyed diff:\n{}", dash_js);

    let dash_wxml = fs::read_to_string(out.join("pages/dashboard/dashboard.wxml")).unwrap();
    assert!(
        dash_wxml.contains("wx:for-index=\"i\""),
        "movers rank must use wx:for-index:\n{}",
        dash_wxml
    );

    for page in ["dashboard", "holdings", "position", "alerts"] {
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

    let app_wxss = fs::read_to_string(out.join("app.wxss")).unwrap();
    assert!(app_wxss.contains(".pressed"), "app.wxss missing .pressed:\n{}", app_wxss);

    for pure in ["stat-card", "section-header", "tag", "empty-state", "allocation-bar", "spark-bars", "cell"] {
        let wxss = out.join(format!("components/{}/{}.wxss", pure, pure));
        assert!(!wxss.exists(), "pure component emitted wxss: {}", wxss.display());
    }
    assert!(out.join("components/stepper/stepper.js").exists(), "stepper must stay a real component");

    let boot = format!(
        r#"
global.App = () => {{}};
global.Page = (o) => {{ global.__page = o; }};
global.Component = () => {{}};
global.wx = {{
  getStorageSync: () => undefined,
  setStorageSync: () => {{}},
  onAppHide: () => {{}},
  stopPullDownRefresh: () => {{}},
  switchTab: () => {{}},
}};
require('{}');
const page = global.__page;
page.setData = function (patch) {{ Object.assign(this.data, patch); }};
page.onLoad({{}});
setTimeout(() => {{
  const d = page.data;
  if (d.totalValue !== 1976250) throw new Error('totalValue wrong: ' + d.totalValue);
  if (d.totalCost !== 1944600) throw new Error('totalCost wrong: ' + d.totalCost);
  if (d.headline.pnl !== '+316.50') throw new Error('headline pnl wrong: ' + d.headline.pnl);
  if (d.headline.tone !== 'up') throw new Error('headline tone wrong: ' + d.headline.tone);
  if (d.allocation.length !== 4) throw new Error('allocation sectors wrong: ' + d.allocation.length);
  if (d.allocation[0].name !== '新能源') throw new Error('allocation order wrong: ' + d.allocation[0].name);
  if (d.movers[0].id !== 'smic') throw new Error('top mover wrong: ' + d.movers[0].id);
  if (d.spark.length !== 6) throw new Error('spark seed wrong: ' + d.spark.length);
  if (d.alertCount !== 2) throw new Error('alertCount wrong: ' + d.alertCount);
  page.refresh();
  setTimeout(() => {{
    if (page.data.totalValue !== 1983620) throw new Error('post-tick totalValue wrong: ' + page.data.totalValue);
    if (page.data.spark.length !== 7) throw new Error('post-tick spark wrong: ' + page.data.spark.length);
    if (page.data.tickLabel !== '第 1 次刷新') throw new Error('tickLabel wrong: ' + page.data.tickLabel);
    if (page.data.totalPnl !== page.data.totalValue - page.data.totalCost) throw new Error('DAG inconsistent after tick');
    console.log('BOOT OK');
  }}, 10);
}}, 10);
"#,
        out.join("pages/dashboard/dashboard.js").display()
    );
    let node = Command::new("node").arg("-e").arg(&boot).output();
    match node {
        Ok(o) => {
            assert!(
                o.status.success() && String::from_utf8_lossy(&o.stdout).contains("BOOT OK"),
                "dashboard.js boot failed:\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
        }
        Err(_) => eprintln!("skipping node boot: node not available"),
    }

    let pos_boot = format!(
        r#"
global.App = () => {{}};
global.Page = (o) => {{ global.__page = o; }};
global.Component = () => {{}};
global.wx = {{
  getStorageSync: () => undefined,
  setStorageSync: () => {{}},
  onAppHide: () => {{}},
}};
require('{}');
const page = global.__page;
page.setData = function (patch) {{ Object.assign(this.data, patch); }};
page.onLoad({{ id: 'catl' }});
if (page.data.missing) throw new Error('missing flag must stay false before first flush');
setTimeout(() => {{
  const d = page.data;
  if (d.missing) throw new Error('catl must not be missing');
  if (d.pos.name !== '宁德时代') throw new Error('pos name wrong: ' + d.pos.name);
  if (d.pos.worthStr !== '3972.00') throw new Error('pos worth wrong: ' + d.pos.worthStr);
  page.buyOne();
  setTimeout(() => {{
    if (page.data.pos.qtyStr !== '21 股') throw new Error('buy did not apply: ' + page.data.pos.qtyStr);
    console.log('POS OK');
  }}, 10);
}}, 10);
"#,
        out.join("pages/position/position.js").display()
    );
    let pos_node = Command::new("node").arg("-e").arg(&pos_boot).output();
    match pos_node {
        Ok(o) => {
            assert!(
                o.status.success() && String::from_utf8_lossy(&o.stdout).contains("POS OK"),
                "position.js boot failed:\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
        }
        Err(_) => eprintln!("skipping node boot: node not available"),
    }
}
