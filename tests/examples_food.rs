use std::fs;
use std::path::Path;
use std::process::Command;

fn build_food() -> std::path::PathBuf {
    let out = std::env::temp_dir().join("mist-food-gate");
    let _ = fs::remove_dir_all(&out);
    let result = Command::new(env!("CARGO_BIN_EXE_mistc"))
        .args(["build", "examples/food/src", "-o"])
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

fn menu_ids(js: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut rest = js;
    while let Some(pos) = rest.find("{ id: '") {
        let tail = &rest[pos + 7..];
        if let Some(end) = tail.find('\'') {
            if tail[end..].starts_with("', cat: '") {
                ids.push(tail[..end].to_string());
            }
        }
        rest = &rest[pos + 7..];
    }
    ids
}

#[test]
fn food_example_passes_all_gates() {
    let out = build_food();

    let cart_js = fs::read_to_string(out.join("stores/cart.js")).unwrap();
    assert!(
        cart_js.contains("rt.store({ lines: [], nextLine: 1 }, { persist: 'food.cart', version: 1 })"),
        "cart persistence missing:\n{}",
        cart_js
    );

    let mut js_files = Vec::new();
    all_js(&out, &mut js_files);
    for (p, js) in &js_files {
        assert!(!js.contains(".this."), "corrupted member call in {}:\n{}", p.display(), js);
    }

    let menu_js = fs::read_to_string(out.join("pages/menu/menu.js")).unwrap();
    let item_js = fs::read_to_string(out.join("pages/item/item.js")).unwrap();
    let item_list = menu_ids(&item_js);
    let menu_list = menu_ids(&menu_js);
    assert!(item_list.len() >= 16, "item catalog too small: {:?}", item_list.len());
    assert_eq!(menu_list, item_list, "MENU copies drifted between menu.mist and item.mist");
    assert!(menu_js.contains("CATS: CATS,"), "sidebar CATS const must seed data:\n{}", menu_js);
    let index_js_seed = fs::read_to_string(out.join("pages/index/index.js")).unwrap();
    assert!(index_js_seed.contains("TILES: TILES,"), "TILES const must seed data:\n{}", index_js_seed);
    assert!(index_js_seed.contains("PICKS: PICKS,"), "PICKS const must seed data:\n{}", index_js_seed);

    for page_path in [
        "pages/index/index.wxml",
        "pages/menu/menu.wxml",
        "pages/item/item.wxml",
        "pages/cart/cart.wxml",
        "packages/order/pages/checkout/checkout.wxml",
        "pages/orders/orders.wxml",
    ] {
        let wxml = fs::read_to_string(out.join(page_path)).unwrap();
        for tag_start in ["<view", "<navigator"] {
            let mut rest = wxml.as_str();
            while let Some(pos) = rest.find(tag_start) {
                let end = tag_end(rest, pos);
                let tag = &rest[pos..end];
                if tag.contains("bindtap") || tag.contains("catchtap") {
                    assert!(
                        tag.contains("hover-class=\"pressed\""),
                        "tappable tag without hover-class in {}:\n{}",
                        page_path,
                        tag
                    );
                }
                rest = &rest[end..];
            }
        }
    }

    let app_wxss = fs::read_to_string(out.join("app.wxss")).unwrap();
    assert!(app_wxss.contains(".pressed"), "app.wxss missing .pressed:\n{}", app_wxss);

    let app_json = fs::read_to_string(out.join("app.json")).unwrap();
    assert!(
        app_json.contains(r#""subPackages": [{ "root": "packages/order", "name": "order", "pages": ["pages/checkout/checkout"] }]"#),
        "app.json missing order subpackage:\n{}",
        app_json
    );
    assert!(
        app_json.contains(r#""preloadRule": { "pages/cart/cart": { "network": "all", "packages": ["order"] } }"#),
        "app.json missing preloadRule:\n{}",
        app_json
    );
    assert!(
        app_json.contains(r#""iconPath": "assets/icons/menu.png""#),
        "app.json missing tab icons:\n{}",
        app_json
    );
    for icon in ["home", "menu", "cart", "orders"] {
        assert!(
            out.join(format!("assets/icons/{}.png", icon)).exists(),
            "missing tab icon asset: {}.png",
            icon
        );
    }
    let sitemap = fs::read_to_string(out.join("sitemap.json")).unwrap();
    assert!(
        sitemap.contains("packages/order/pages/checkout/checkout"),
        "user sitemap not honored:\n{}",
        sitemap
    );

    let app_js = fs::read_to_string(out.join("app.js")).unwrap();
    for hook in ["onError", "onPageNotFound", "onUnhandledRejection", "onThemeChange"] {
        assert!(app_js.contains(hook), "app.js missing {} hook:\n{}", hook, app_js);
    }

    let checkout_js = fs::read_to_string(out.join("packages/order/pages/checkout/checkout.js")).unwrap();
    assert!(
        checkout_js.contains("require('../../../../mist-rt.js')"),
        "subpackage runtime require not depth-aware:\n{}",
        checkout_js
    );

    let menu_js_full = fs::read_to_string(out.join("pages/menu/menu.js")).unwrap();
    assert!(
        menu_js_full.contains("rt.derive(this, __o, 'cartCount'"),
        "menu.js missing cartCount derive:\n{}",
        menu_js_full
    );
    assert!(
        menu_js_full.contains("rt.derive(this, __o, 'cartTotal'"),
        "menu.js missing cartTotal derive:\n{}",
        menu_js_full
    );

    let menu_wxml = fs::read_to_string(out.join("pages/menu/menu.wxml")).unwrap();
    assert!(menu_wxml.contains("去结算"), "menu.wxml missing 去结算 mini cart bar:\n{}", menu_wxml);

    let orders_wxml = fs::read_to_string(out.join("pages/orders/orders.wxml")).unwrap();
    assert!(orders_wxml.contains("mist-pulse"), "orders.wxml missing mist-pulse animation:\n{}", orders_wxml);

    let cart_js_full = fs::read_to_string(out.join("pages/cart/cart.js")).unwrap();
    assert!(
        cart_js_full.contains("rt.derive(this, __o, 'gap'"),
        "cart.js missing gap derive:\n{}",
        cart_js_full
    );

    let index_wxml = fs::read_to_string(out.join("pages/index/index.wxml")).unwrap();
    assert!(index_wxml.contains("swiper"), "index.wxml missing swiper:\n{}", index_wxml);
    assert!(index_wxml.contains("scroll-x"), "index.wxml missing scroll-x rail:\n{}", index_wxml);
    assert!(
        index_wxml.contains("linear-gradient(135deg"),
        "index.wxml missing hero gradient:\n{}",
        index_wxml
    );

    let index_js = fs::read_to_string(out.join("pages/index/index.js")).unwrap();
    assert!(
        index_js.contains("rt.derive(this, __o, 'cartInfo'"),
        "index.js missing cartInfo derive:\n{}",
        index_js
    );
    assert!(
        index_js.contains("rt.derive(this, __o, 'making'"),
        "index.js missing making derive:\n{}",
        index_js
    );

    for pure in ["cell", "section-header", "tag", "price-tag", "empty-state", "menu-item"] {
        let wxss = out.join(format!("components/{}/{}.wxss", pure, pure));
        assert!(!wxss.exists(), "pure component emitted wxss: {}", wxss.display());
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
  showToast: () => {{}},
  switchTab: () => {{}},
}};
require('{}');
const page = global.__page;
page.setData = () => {{}};
page.onLoad({{ id: 'latte' }});
setTimeout(() => {{
  if (page.data.unit !== 18) throw new Error('latte unit price wrong: ' + page.data.unit);
  if (page.data.groups.length !== 3) throw new Error('option groups missing: ' + page.data.groups.length);
  if (!page.data.groups[0].choices[0].picked) throw new Error('default selection missing');
  console.log('BOOT OK');
}}, 10);
"#,
        out.join("pages/item/item.js").display()
    );
    let node = Command::new("node").arg("-e").arg(&boot).output();
    match node {
        Ok(o) => {
            assert!(
                o.status.success() && String::from_utf8_lossy(&o.stdout).contains("BOOT OK"),
                "item.js boot failed:\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
        }
        Err(_) => eprintln!("skipping node boot: node not available"),
    }
}
