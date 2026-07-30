use std::path::Path;

fn build() -> mistc::Project {
    mistc::compile_project_dir(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/project/src"
    )))
    .expect("project dir compile failed")
}

#[test]
fn store_module_compiles_to_runtime_store() {
    let p = build();
    let store = p.files.iter().find(|f| f.out_path == "stores/stats").expect("store not compiled");
    let js = &store.output.js;
    assert!(js.contains("const rt = require('../mist-rt.js');"), "js:\n{}", js);
    assert!(js.contains("const stats = rt.store({ taps: 0, lastAction: 'none' });"), "js:\n{}", js);
    // mutations inside store functions are path-precise
    assert!(js.contains("stats.__set(`taps`, stats.value.taps + 1)"), "js:\n{}", js);
    assert!(js.contains("stats.__set(`lastAction`, action)"), "js:\n{}", js);
    assert!(js.contains("module.exports = { stats, track };"), "js:\n{}", js);
    // stores emit no wxml/wxss
    assert!(store.output.wxml.is_empty());
}

#[test]
fn page_wires_store_require_mirror_and_lifecycle() {
    let p = build();
    let index = p.files.iter().find(|f| f.out_path == "pages/index/index").unwrap();
    let js = &index.output.js;
    assert!(js.contains("const __S0 = require('../../stores/stats.js');"), "js:\n{}", js);
    // mirror key in data
    assert!(js.contains("stats: null"), "js:\n{}", js);
    // subscription lifecycle
    assert!(js.contains("rt.bindStores(this, [[__S0.stats, 'stats']]);"), "js:\n{}", js);
    assert!(js.contains("onUnload() {\n    rt.unbindStores(this);\n  }"), "js:\n{}", js);
    // imported store fn gets a wrapper method, so template events and methods work
    assert!(js.contains("track(...args) {\n    return __S0.track(...args);\n  }"), "js:\n{}", js);
    // method call rewritten through the wrapper
    assert!(js.contains("this.track('toggle')"), "js:\n{}", js);
}

#[test]
fn template_binds_store_mirror() {
    let p = build();
    let index = p.files.iter().find(|f| f.out_path == "pages/index/index").unwrap();
    assert!(index.output.wxml.contains("{{stats.taps}}"), "wxml:\n{}", index.output.wxml);
    let about = p.files.iter().find(|f| f.out_path == "pages/about/about").unwrap();
    assert!(about.output.wxml.contains("{{stats.taps}}"), "wxml:\n{}", about.output.wxml);
    assert!(about.output.wxml.contains("{{stats.lastAction}}"), "wxml:\n{}", about.output.wxml);
}

#[test]
fn store_module_is_compiled_once_for_many_pages() {
    let p = build();
    let count = p.files.iter().filter(|f| f.out_path == "stores/stats").count();
    assert_eq!(count, 1);
}

#[test]
fn page_side_store_writes_are_path_precise() {
    let dir = std::env::temp_dir().join("mist-store-writes");
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("stores.ts"),
        "import { store } from 'mist'\nexport const cart = store({ items: [], total: 0 })\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { cart } from '../stores.ts'\nimport { state } from 'mist'\nconst n = state(0)\nfunction add(item) {\n  cart.value.items.push(item)\n  cart.value.total += item.price\n}\nfunction clear() {\n  cart.value = { items: [], total: 0 }\n}\n---\n<span onTap={clear}>{cart.value.total} {n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(dir.join("app.mist"), "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n").unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    let index = p.files.iter().find(|f| f.is_page).unwrap();
    let js = &index.output.js;
    assert!(
        js.contains("__S0.cart.__set(`items[${__S0.cart.value.items.length}]`, item)"),
        "js:\n{}",
        js
    );
    assert!(
        js.contains("__S0.cart.__set(`total`, __S0.cart.value.total + (item.price))"),
        "js:\n{}",
        js
    );
    assert!(js.contains("__S0.cart.__set(null, { items: [], total: 0 })"), "js:\n{}", js);
}

#[test]
fn plain_value_store_export_errors() {
    let err = mistc::frontmatter::compile_store_module(
        "import { store } from 'mist'\nexport const s = store(1)\nexport const LIMIT = 5\n",
        "./mist-rt.js",
    )
    .unwrap_err();
    assert!(err.contains("plain values"), "err: {}", err);
}

#[test]
fn arrow_function_store_export_is_callable() {
    let (js, info) = mistc::frontmatter::compile_store_module(
        "import { store } from 'mist'\nexport const s = store(1)\nexport const reset = () => { s.value = 0 }\n",
        "./mist-rt.js",
    )
    .expect("compile failed");
    assert!(info.fns.contains(&"reset".to_string()), "info fns: {:?}", info.fns);
    assert!(js.contains("s.__set(null, 0)"), "js:\n{}", js);
}

#[test]
fn store_name_colliding_with_state_errors() {
    let dir = std::env::temp_dir().join("mist-store-collide");
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(dir.join("s.ts"), "import { store } from 'mist'\nexport const stats = store(0)\n").unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { stats } from '../s.ts'\nimport { state } from 'mist'\nconst stats = state(0)\n---\n<span>{stats.value}</span>\n",
    )
    .unwrap();
    std::fs::write(dir.join("app.mist"), "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n").unwrap();
    let err = mistc::compile_project_dir(&dir).unwrap_err();
    assert!(err.contains("M1005"), "err: {}", err);
}

#[test]
fn same_stem_store_files_collide_with_clear_error() {
    let dir = std::env::temp_dir().join("mist-store-stems");
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::create_dir_all(dir.join("a")).unwrap();
    std::fs::create_dir_all(dir.join("b")).unwrap();
    std::fs::write(dir.join("a/user.ts"), "import { store } from 'mist'\nexport const userA = store(1)\n").unwrap();
    std::fs::write(dir.join("b/user.ts"), "import { store } from 'mist'\nexport const userB = store(2)\n").unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { userA } from '../a/user.ts'\nimport { userB } from '../b/user.ts'\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{userA.value} {userB.value} {n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(dir.join("app.mist"), "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n").unwrap();
    let err = mistc::compile_project_dir(&dir).unwrap_err();
    assert!(err.contains("collision"), "err: {}", err);
}

#[test]
fn unknown_store_export_errors() {
    let dir = std::env::temp_dir().join("mist-store-unknown");
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(dir.join("s.ts"), "import { store } from 'mist'\nexport const a = store(1)\n").unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { nope } from '../s.ts'\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(dir.join("app.mist"), "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n").unwrap();
    let err = mistc::compile_project_dir(&dir).unwrap_err();
    assert!(err.contains("not exported"), "err: {}", err);
}
