use std::path::Path;

fn build() -> mistc::Project {
    mistc::compile_project_dir(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/project/src"
    )))
    .expect("project dir compile failed")
}

#[test]
fn nested_layout_paths() {
    let p = build();
    let paths: Vec<&str> = p.files.iter().map(|f| f.out_path.as_str()).collect();
    assert!(paths.contains(&"pages/index/index"), "paths: {:?}", paths);
    assert!(paths.contains(&"pages/about/about"), "paths: {:?}", paths);
    assert!(paths.contains(&"components/todo-item/todo-item"), "paths: {:?}", paths);
    assert!(paths.contains(&"components/badge/badge"), "paths: {:?}", paths);
}

#[test]
fn app_shell_generated_from_app_mist() {
    let p = build();
    let app = p.app.as_ref().expect("missing app shell");
    // index first (launch page), then remaining pages
    assert!(app.json.contains("\"pages\": [\"pages/index/index\", \"pages/about/about\"]"), "json:\n{}", app.json);
    assert!(app.json.contains("\"navigationBarTitleText\": \"Mist\""), "json:\n{}", app.json);
    assert!(app.js.contains("App({"), "js:\n{}", app.js);
    // no trailing commas may survive the object-literal → JSON conversion
    assert!(!app.json.contains(",,"), "json:\n{}", app.json);
    assert!(!regex_lite_contains(&app.json), "json:\n{}", app.json);
    assert!(app.js.contains("onLaunch()"), "js:\n{}", app.js);
    assert!(app.wxss.contains("page { background: #f9fafb; }"), "wxss:\n{}", app.wxss);
}

/// true when a `,` directly precedes `}` or `]` (ignoring whitespace)
fn regex_lite_contains(json: &str) -> bool {
    let mut prev_comma = false;
    for c in json.chars() {
        match c {
            ',' => prev_comma = true,
            '}' | ']' if prev_comma => return true,
            c if c.is_whitespace() => {}
            _ => prev_comma = false,
        }
    }
    false
}

#[test]
fn nested_paths_in_requires_imports_and_using_components() {
    let p = build();
    let index = p.files.iter().find(|f| f.out_path == "pages/index/index").unwrap();
    assert!(index.output.js.contains("require('../../mist-rt.js')"), "js:\n{}", index.output.js);
    assert!(index.output.wxss.contains("@import \"../../tw-theme.wxss\";"), "wxss:\n{}", index.output.wxss);
    assert!(index.output.wxss.contains("@import \"../../tw-shared.wxss\";"), "wxss:\n{}", index.output.wxss);
    // components must NOT import the theme sheet (page selector is illegal there)
    let comp = p.files.iter().find(|f| f.out_path == "components/todo-item/todo-item").unwrap();
    assert!(!comp.output.wxss.contains("tw-theme"), "wxss:\n{}", comp.output.wxss);
    // and the shared utility sheet must not contain the page selector
    assert!(!p.tailwind_css.contains("page {"), "css:\n{}", p.tailwind_css);
    assert!(p.tailwind_theme_css.contains("page {"), "theme:\n{}", p.tailwind_theme_css);
    assert!(
        index.output.wxml.contains("<import src=\"../../components/badge/badge.wxml\" />"),
        "wxml:\n{}",
        index.output.wxml
    );
    let json = index.output.json.as_deref().unwrap();
    assert!(
        json.contains("\"todo-item\": \"../../components/todo-item/todo-item\""),
        "json:\n{}",
        json
    );

    let comp = p.files.iter().find(|f| f.out_path == "components/todo-item/todo-item").unwrap();
    assert!(comp.output.js.contains("require('../../mist-rt.js')"), "js:\n{}", comp.output.js);
}

#[test]
fn config_strings_with_commas_and_colons_survive() {
    let src = "---\nimport { state } from 'mist'\nexport const config = { navigationBarTitleText: 'Home, tab: one' }\nconst n = state(0)\n---\n<span>{n.value}</span>\n";
    let out = mistc::compile(src).expect("compile failed");
    let json = out.json.expect("missing json");
    assert!(json.contains("\"navigationBarTitleText\": \"Home, tab: one\""), "json:\n{}", json);
}

#[test]
fn config_rejects_non_literals() {
    let src = "---\nimport { state } from 'mist'\nexport const config = { title: getTitle() }\nconst n = state(0)\n---\n<span>{n.value}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("literal"), "err: {}", err);
}

#[test]
fn app_mist_rejects_state_and_templates() {
    let dir = std::env::temp_dir().join("mist-app-guards");
    let pages = dir.join("pages");
    std::fs::create_dir_all(&pages).unwrap();
    std::fs::write(pages.join("index.mist"), "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n").unwrap();

    std::fs::write(dir.join("app.mist"), "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n").unwrap();
    let err = mistc::compile_project_dir(&dir).unwrap_err();
    assert!(err.contains("cannot declare state"), "err: {}", err);

    std::fs::write(dir.join("app.mist"), "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n<div>oops</div>\n").unwrap();
    let err = mistc::compile_project_dir(&dir).unwrap_err();
    assert!(err.contains("cannot have a template"), "err: {}", err);
}

#[test]
fn anchor_compiles_to_navigator_with_url() {
    let p = build();
    let index = p.files.iter().find(|f| f.out_path == "pages/index/index").unwrap();
    assert!(
        index.output.wxml.contains("<navigator url=\"/pages/about/about\""),
        "wxml:\n{}",
        index.output.wxml
    );
}

#[test]
fn second_page_compiles_with_own_state() {
    let p = build();
    let about = p.files.iter().find(|f| f.out_path == "pages/about/about").unwrap();
    assert!(about.output.js.contains("visits: 0"), "js:\n{}", about.output.js);
    assert!(about.output.js.contains("this.__set('visits', this.data.visits + 1)"), "js:\n{}", about.output.js);
    assert!(about.output.wxml.contains("{{visits}}"), "wxml:\n{}", about.output.wxml);
}

#[test]
fn tab_bar_config_reaches_app_json() {
    let dir = std::env::temp_dir().join("mist-tabbar");
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(dir.join("pages/index.mist"), "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n").unwrap();
    std::fs::write(dir.join("pages/stats.mist"), "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n").unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nexport const config = {\n  tabBar: {\n    color: '#9ca3af',\n    selectedColor: '#155dfc',\n    list: [\n      { pagePath: 'pages/index/index', text: 'Home' },\n      { pagePath: 'pages/stats/stats', text: 'Stats' },\n    ],\n  },\n}\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    let app = p.app.as_ref().unwrap();
    assert!(app.json.contains("\"tabBar\""), "json:\n{}", app.json);
    assert!(app.json.contains("\"pagePath\": \"pages/stats/stats\""), "json:\n{}", app.json);
    assert!(app.json.contains("\"selectedColor\": \"#155dfc\""), "json:\n{}", app.json);
}

#[test]
fn app_hooks_include_error_and_theme_change() {
    let dir = std::env::temp_dir().join("mist-app-error-theme");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(dir.join("pages/index.mist"), "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n").unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch, onError, onThemeChange } from 'mist'\nonLaunch(() => {})\nonError((error) => { console.log(error) })\nonThemeChange((e) => { console.log(e.theme) })\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    let app = p.app.as_ref().unwrap();
    assert!(app.js.contains("onError(error)"), "js:\n{}", app.js);
    assert!(app.js.contains("onThemeChange(e)"), "js:\n{}", app.js);
}

#[test]
fn dropped_pages_in_subdir_warn_m1016() {
    let dir = std::env::temp_dir().join("mist-dropped-pages-subdir");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages/sub")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pages/sub/x.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    assert!(
        p.warnings.iter().any(|w| w.contains("M1016") && w.contains("sub")),
        "warnings: {:?}",
        p.warnings
    );
    let paths: Vec<&str> = p.files.iter().map(|f| f.out_path.as_str()).collect();
    assert!(!paths.iter().any(|p| p.contains("sub")), "paths: {:?}", paths);
}

#[test]
fn subpackage_page_compiles_at_depth_four() {
    let dir = std::env::temp_dir().join("mist-subpkg");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::create_dir_all(dir.join("packages/shop/pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("packages/shop/pages/cart.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span class=\"p-4\">{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    let cart = p.files.iter().find(|f| f.out_path == "packages/shop/pages/cart/cart").unwrap();
    assert!(cart.output.js.contains("require('../../../../mist-rt.js')"), "js:\n{}", cart.output.js);
    assert!(
        cart.output.wxss.contains("@import \"../../../../tw-shared.wxss\";"),
        "wxss:\n{}",
        cart.output.wxss
    );
    assert_eq!(cart.package.as_deref(), Some("shop"));
}

#[test]
fn subpackage_page_shares_main_package_component_and_store() {
    let dir = std::env::temp_dir().join("mist-subpkg-shared-component-store");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::create_dir_all(dir.join("components")).unwrap();
    std::fs::create_dir_all(dir.join("stores")).unwrap();
    std::fs::create_dir_all(dir.join("packages/shop/pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport Item from '../components/Item.mist'\nimport { state } from 'mist'\nconst n = state(0)\nfunction go() {}\n---\n<Item count={n.value} onGo={go} />\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("components/Item.mist"),
        "---\nimport { props } from 'mist'\nconst { count, onGo } = props({ count: 0 })\n---\n<span onTap={onGo}>{count}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("stores/stats.ts"),
        "import { store } from 'mist'\n\nexport const stats = store({ taps: 0 })\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("packages/shop/pages/cart.mist"),
        "---\nimport Item from '../../../components/Item.mist'\nimport { stats } from '../../../stores/stats.ts'\nimport { state } from 'mist'\nconst n = state(0)\nfunction go() {}\n---\n<Item count={n.value + stats.value.taps} onGo={go} />\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");

    let cart = p.files.iter().find(|f| f.out_path == "packages/shop/pages/cart/cart").unwrap();
    let cart_json = cart.output.json.as_deref().unwrap();
    assert!(
        cart_json.contains("\"../../../../components/item/item\""),
        "json:\n{}",
        cart_json
    );
    assert!(
        cart.output.js.contains("require('../../../../stores/stats.js')"),
        "js:\n{}",
        cart.output.js
    );
    assert!(
        cart.output.js.contains("require('../../../../mist-rt.js')"),
        "js:\n{}",
        cart.output.js
    );

    let item_count = p.files.iter().filter(|f| f.out_path == "components/item/item").count();
    assert_eq!(item_count, 1, "component must compile exactly once, at its main-package path");

    let index = p.files.iter().find(|f| f.out_path == "pages/index/index").unwrap();
    let index_json = index.output.json.as_deref().unwrap();
    assert!(
        index_json.contains("\"../../components/item/item\""),
        "json:\n{}",
        index_json
    );
}

#[test]
fn dropped_pages_in_subpackage_subdir_warn_m1016() {
    let dir = std::env::temp_dir().join("mist-subpkg-dropped-pages-subdir");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::create_dir_all(dir.join("packages/shop/pages/nested")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("packages/shop/pages/cart.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("packages/shop/pages/nested/deep.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    assert!(
        p.warnings.iter().any(|w| {
            w.contains("M1016") && w.contains("packages/shop/pages/nested/")
        }),
        "warnings: {:?}",
        p.warnings
    );
    let paths: Vec<&str> = p.files.iter().map(|f| f.out_path.as_str()).collect();
    assert!(!paths.iter().any(|p| p.contains("nested")), "paths: {:?}", paths);
}

#[test]
fn reserved_package_name_errors() {
    let dir = std::env::temp_dir().join("mist-subpkg-reserved-name");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::create_dir_all(dir.join("packages/components/pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("packages/components/pages/cart.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let err = mistc::compile_project_dir(&dir).unwrap_err();
    assert!(err.contains("reserved"), "err: {}", err);
}

#[test]
fn invalid_package_name_errors() {
    let dir = std::env::temp_dir().join("mist-subpkg-bad-name");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::create_dir_all(dir.join("packages/shop cart/pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("packages/shop cart/pages/cart.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let err = mistc::compile_project_dir(&dir).unwrap_err();
    assert!(err.contains("invalid subpackage name"), "err: {}", err);
}

#[test]
fn main_package_requires_at_least_one_page() {
    let dir = std::env::temp_dir().join("mist-subpkg-only");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::create_dir_all(dir.join("packages/shop/pages")).unwrap();
    std::fs::write(
        dir.join("packages/shop/pages/cart.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let err = mistc::compile_project_dir(&dir).unwrap_err();
    assert!(err.contains("main package needs at least one page"), "err: {}", err);
    assert!(err.contains("no pages found"), "err: {}", err);
}

#[test]
fn sub_packages_emitted_and_split_from_main_pages() {
    let dir = std::env::temp_dir().join("mist-subpkg-app-json");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::create_dir_all(dir.join("packages/shop/pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("packages/shop/pages/cart.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    let app = p.app.as_ref().unwrap();
    assert!(
        app.json.contains("\"subPackages\": [{ \"root\": \"packages/shop\", \"name\": \"shop\", \"pages\": [\"pages/cart/cart\"] }]"),
        "json:\n{}",
        app.json
    );
    assert!(!app.json.contains("packages/shop/pages/cart/cart"), "json:\n{}", app.json);
    assert!(app.json.contains("\"pages\": [\"pages/index/index\"]"), "json:\n{}", app.json);
}

#[test]
fn sub_packages_key_in_user_config_errors_m1014() {
    let dir = std::env::temp_dir().join("mist-subpkg-reserved-key");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nexport const config = { subPackages: [] }\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let err = mistc::compile_project_dir(&dir).unwrap_err();
    assert!(err.contains("M1014"), "err: {}", err);
    assert!(err.contains("subPackages"), "err: {}", err);
}

#[test]
fn preload_rule_passes_through_to_app_json() {
    let dir = std::env::temp_dir().join("mist-preload-rule");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::create_dir_all(dir.join("packages/shop/pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("packages/shop/pages/cart.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nexport const config = {\n  preloadRule: {\n    'pages/index/index': { network: 'all', packages: ['shop'] },\n  },\n}\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    let app = p.app.as_ref().unwrap();
    assert!(app.json.contains("\"preloadRule\""), "json:\n{}", app.json);
    assert!(app.json.contains("\"network\": \"all\""), "json:\n{}", app.json);
}

/// Escape hatch: a page with no .mist component imports may still hand-register
/// native components via a manual `usingComponents` — that must keep working.
#[test]
fn manual_using_components_without_imports_still_compiles() {
    let dir = std::env::temp_dir().join("mist-manual-using-components");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\nexport const config = { usingComponents: { 'van-button': '@vant/weapp/button/index' } }\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    let index = p.files.iter().find(|f| f.out_path == "pages/index/index").unwrap();
    let json = index.output.json.as_deref().unwrap();
    let count = json.matches("\"usingComponents\"").count();
    assert_eq!(count, 1, "json:\n{}", json);
    assert!(json.contains("\"van-button\": \"@vant/weapp/button/index\""), "json:\n{}", json);
    assert!(
        p.warnings.iter().all(|w| !w.contains("M1019")),
        "warnings: {:?}",
        p.warnings
    );
}

#[test]
fn custom_tab_bar_compiles_at_fixed_dist_path() {
    let dir = std::env::temp_dir().join("mist-custom-tab-bar-ok");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("custom-tab-bar.mist"),
        "---\nimport { state } from 'mist'\nconst active = state(0)\n---\n<div>{active.value}</div>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nexport const config = {\n  tabBar: {\n    custom: true,\n    list: [{ pagePath: 'pages/index/index', text: 'Home' }],\n  },\n}\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    let tab_bar = p.files.iter().find(|f| f.out_path == "custom-tab-bar/index").expect("missing custom-tab-bar/index");
    let json = tab_bar.output.json.as_deref().expect("missing json");
    assert!(json.contains("\"component\": true"), "json:\n{}", json);
    assert!(tab_bar.output.js.contains("require('../mist-rt.js')"), "js:\n{}", tab_bar.output.js);
    assert!(!tab_bar.output.wxml.is_empty(), "wxml empty");
    assert!(
        p.warnings.iter().all(|w| !w.contains("M1020")),
        "warnings: {:?}",
        p.warnings
    );
}

#[test]
fn custom_tab_bar_flag_without_file_is_m1020_error() {
    let dir = std::env::temp_dir().join("mist-custom-tab-bar-missing-file");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nexport const config = {\n  tabBar: {\n    custom: true,\n    list: [{ pagePath: 'pages/index/index', text: 'Home' }],\n  },\n}\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let err = mistc::compile_project_dir(&dir).unwrap_err();
    assert!(err.contains("M1020"), "err: {}", err);
}

#[test]
fn custom_tab_bar_file_without_flag_is_m1020_warning() {
    let dir = std::env::temp_dir().join("mist-custom-tab-bar-missing-flag");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("custom-tab-bar.mist"),
        "---\nimport { state } from 'mist'\nconst active = state(0)\n---\n<div>{active.value}</div>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    assert!(
        p.warnings.iter().any(|w| w.contains("M1020")),
        "warnings: {:?}",
        p.warnings
    );
    let paths: Vec<&str> = p.files.iter().map(|f| f.out_path.as_str()).collect();
    assert!(paths.contains(&"custom-tab-bar/index"), "paths: {:?}", paths);
}

#[test]
fn custom_tab_bar_imports_shared_component_and_store() {
    let dir = std::env::temp_dir().join("mist-custom-tab-bar-imports");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::create_dir_all(dir.join("components")).unwrap();
    std::fs::create_dir_all(dir.join("stores")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("components/TabIcon.mist"),
        "---\nimport { props } from 'mist'\nconst { label, onPick } = props({ label: 'x' })\n---\n<div onTap={onPick}>{label}</div>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("stores/tabstate.ts"),
        "import { store } from 'mist'\nexport const active = store({ index: 0 })\nexport function setActive(i) {\n  active.value.index = i\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("custom-tab-bar.mist"),
        "---\nimport TabIcon from './components/TabIcon.mist'\nimport { active, setActive } from './stores/tabstate.ts'\n---\n<div>\n  <TabIcon label=\"home\" onPick={() => setActive(0)} />\n  <span>{active.value.index}</span>\n</div>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nexport const config = {\n  tabBar: {\n    custom: true,\n    list: [{ pagePath: 'pages/index/index', text: 'Home' }],\n  },\n}\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    let tab_bar = p.files.iter().find(|f| f.out_path == "custom-tab-bar/index").expect("missing custom-tab-bar/index");
    let json = tab_bar.output.json.as_deref().expect("missing json");
    assert!(
        json.contains("\"tab-icon\": \"../components/tab-icon/tab-icon\""),
        "json:\n{}",
        json
    );
    assert!(tab_bar.output.js.contains("require('../stores/tabstate.js')"), "js:\n{}", tab_bar.output.js);

    let comp_paths: Vec<&str> = p.files.iter().map(|f| f.out_path.as_str()).collect();
    assert_eq!(
        comp_paths.iter().filter(|p| **p == "components/tab-icon/tab-icon").count(),
        1,
        "paths: {:?}",
        comp_paths
    );
    assert_eq!(
        comp_paths.iter().filter(|p| **p == "stores/tabstate").count(),
        1,
        "paths: {:?}",
        comp_paths
    );
}

#[test]
fn custom_tab_bar_rejects_page_lifecycle_hook() {
    let dir = std::env::temp_dir().join("mist-custom-tab-bar-onload");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("custom-tab-bar.mist"),
        "---\nimport { onLoad } from 'mist'\nonLoad(() => {})\n---\n<div>tab bar</div>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nexport const config = {\n  tabBar: {\n    custom: true,\n    list: [{ pagePath: 'pages/index/index', text: 'Home' }],\n  },\n}\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let err = mistc::compile_project_dir(&dir).unwrap_err();
    assert!(err.contains("M1013"), "err: {}", err);
    assert!(err.contains("onAttach"), "err: {}", err);
}

#[test]
fn navigate_to_known_route_compiles_clean() {
    let dir = std::env::temp_dir().join("mist-navigate-known-route");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { navigate } from 'mist'\nfunction go() {\n  navigate('/pages/about/about')\n}\n---\n<div onTap={go}>go</div>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pages/about.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    let index = p.files.iter().find(|f| f.out_path == "pages/index/index").unwrap();
    assert!(
        index.output.js.contains("wx.navigateTo({ url: '/pages/about/about' })"),
        "js:\n{}",
        index.output.js
    );
    assert!(p.warnings.iter().all(|w| !w.contains("M1021")), "warnings: {:?}", p.warnings);
}

#[test]
fn navigate_to_unknown_route_errors_m1021_with_suggestion() {
    let dir = std::env::temp_dir().join("mist-navigate-unknown-route");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { navigate } from 'mist'\nfunction go() {\n  navigate('/pages/abot/abot')\n}\n---\n<div onTap={go}>go</div>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pages/about.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let err = mistc::compile_project_dir(&dir).unwrap_err();
    assert!(err.contains("M1021"), "err: {}", err);
    assert!(err.contains("/pages/abot/abot"), "err: {}", err);
    assert!(err.contains("did you mean '/pages/about/about'?"), "err: {}", err);
}

#[test]
fn navigate_to_subpackage_route_compiles_clean() {
    let dir = std::env::temp_dir().join("mist-navigate-subpkg-route");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::create_dir_all(dir.join("packages/shop/pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { navigate } from 'mist'\nfunction go() {\n  navigate('/packages/shop/pages/cart/cart')\n}\n---\n<div onTap={go}>go</div>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("packages/shop/pages/cart.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span class=\"p-4\">{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    assert!(p.warnings.iter().all(|w| !w.contains("M1021")), "warnings: {:?}", p.warnings);
}

#[test]
fn navigate_switch_tab_to_known_tab_page_compiles_clean() {
    let dir = std::env::temp_dir().join("mist-navigate-switch-tab-known");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { navigate } from 'mist'\nfunction go() {\n  navigate.switchTab('/pages/about/about')\n}\n---\n<div onTap={go}>go</div>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pages/about.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nexport const config = {\n  tabBar: {\n    list: [\n      { pagePath: 'pages/index/index', text: 'Home' },\n      { pagePath: 'pages/about/about', text: 'About' },\n    ],\n  },\n}\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    assert!(p.warnings.iter().all(|w| !w.contains("M1021")), "warnings: {:?}", p.warnings);
}

#[test]
fn navigate_switch_tab_to_non_tab_page_errors_m1021_variant() {
    let dir = std::env::temp_dir().join("mist-navigate-switch-tab-non-tab");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { navigate } from 'mist'\nfunction go() {\n  navigate.switchTab('/pages/about/about')\n}\n---\n<div onTap={go}>go</div>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pages/about.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nexport const config = {\n  tabBar: {\n    list: [\n      { pagePath: 'pages/index/index', text: 'Home' },\n    ],\n  },\n}\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let err = mistc::compile_project_dir(&dir).unwrap_err();
    assert!(err.contains("M1021"), "err: {}", err);
    assert!(err.contains("not a tab-bar page"), "err: {}", err);
}

#[test]
fn navigate_non_literal_route_errors() {
    let dir = std::env::temp_dir().join("mist-navigate-non-literal");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { navigate } from 'mist'\nconst target = '/pages/index/index'\nfunction go() {\n  navigate(target)\n}\n---\n<div onTap={go}>go</div>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let err = mistc::compile_project_dir(&dir).unwrap_err();
    assert!(err.contains("M1021"), "err: {}", err);
    assert!(err.contains("literal strings"), "err: {}", err);
}

#[test]
fn navigate_inside_lifecycle_body_rewrites() {
    let dir = std::env::temp_dir().join("mist-navigate-lifecycle");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { onLoad, navigate } from 'mist'\nonLoad(() => {\n  navigate.replace('/pages/about/about')\n})\n---\n<span>hi</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pages/about.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    let index = p.files.iter().find(|f| f.out_path == "pages/index/index").unwrap();
    assert!(
        index.output.js.contains("wx.redirectTo({ url: '/pages/about/about' })"),
        "js:\n{}",
        index.output.js
    );
}

#[test]
fn m1002_silent_for_class_defined_in_own_style_block() {
    let dir = std::env::temp_dir().join("mist-m1002-own-style");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<div class=\"card\">{n.value}</div>\n\n<style>\n.card { border-radius: 8px; }\n</style>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    assert!(
        !p.unknown_classes.iter().any(|c| c == "card"),
        "unknown: {:?}",
        p.unknown_classes
    );
}

#[test]
fn m1002_silent_for_class_defined_in_app_style_block() {
    let dir = std::env::temp_dir().join("mist-m1002-app-style");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<div class=\"hero\">{n.value}</div>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n\n<style>\n.hero { padding: 4px; }\n</style>\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    assert!(
        !p.unknown_classes.iter().any(|c| c == "hero"),
        "unknown: {:?}",
        p.unknown_classes
    );
}

#[test]
fn m1002_still_fires_for_genuinely_undefined_class() {
    let dir = std::env::temp_dir().join("mist-m1002-still-fires");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<div class=\"totally-undefined-thing\">{n.value}</div>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    assert!(
        p.unknown_classes.iter().any(|c| c == "totally-undefined-thing"),
        "unknown: {:?}",
        p.unknown_classes
    );
}

#[test]
fn m1002_silent_for_class_defined_only_inside_media_query() {
    let dir = std::env::temp_dir().join("mist-m1002-media-style");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<div class=\"wide\">{n.value}</div>\n\n<style>\n@media (min-width: 750px) {\n  .wide { width: 100%; }\n}\n</style>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    assert!(
        !p.unknown_classes.iter().any(|c| c == "wide"),
        "unknown: {:?}",
        p.unknown_classes
    );
}

#[test]
fn app_style_rejects_scoped() {
    let dir = std::env::temp_dir().join("mist-app-scoped");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(dir.join("pages/index.mist"), "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n").unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n<style scoped>\npage { background: red; }\n</style>\n",
    )
    .unwrap();
    let err = mistc::compile_project_dir(&dir).unwrap_err();
    assert!(err.contains("cannot be scoped"), "err: {}", err);
}

#[test]
fn route_param_page_compiles_with_guard_and_seed() {
    let dir = std::env::temp_dir().join("mist-route-param");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages/item")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pages/item/[id].mist"),
        "---\nimport { state } from 'mist'\nconst id = state('')\n---\n<span>{id.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    assert!(p.warnings.is_empty(), "warnings: {:?}", p.warnings);
    let item = p.files.iter().find(|f| f.out_path == "pages/item/item").expect("route page missing");
    assert!(item.is_page);
    assert_eq!(item.route_param.as_deref(), Some("id"));
    assert!(item.output.js.contains("onLoad(__q)"), "js:\n{}", item.output.js);
    assert!(item.output.js.contains("__q.id === undefined"), "js:\n{}", item.output.js);
    assert!(item.output.js.contains("wx.navigateBack"), "js:\n{}", item.output.js);
    assert!(item.output.js.contains("this.__set('id', __q.id)"), "js:\n{}", item.output.js);
    let app = p.app.expect("app missing");
    assert!(app.json.contains("pages/item/item"), "app.json:\n{}", app.json);
}

#[test]
fn route_param_page_wraps_user_on_load() {
    let dir = std::env::temp_dir().join("mist-route-param-onload");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages/item")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pages/item/[id].mist"),
        "---\nimport { state, onLoad } from 'mist'\nconst id = state('')\nconst extra = state('')\nonLoad((q) => {\n  extra.value = q.from || ''\n})\n---\n<span>{id.value}{extra.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    let item = p.files.iter().find(|f| f.out_path == "pages/item/item").unwrap();
    assert!(item.output.js.contains("__q.id === undefined"), "js:\n{}", item.output.js);
    assert!(item.output.js.contains("this.__set('id', __q.id)"), "js:\n{}", item.output.js);
    assert!(item.output.js.contains(")(__q);"), "user onLoad must still run:\n{}", item.output.js);
    assert!(item.output.js.contains("q.from"), "user body must survive:\n{}", item.output.js);
}

#[test]
fn route_param_page_requires_matching_state() {
    let dir = std::env::temp_dir().join("mist-route-param-missing-state");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages/item")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pages/item/[id].mist"),
        "---\nimport { state } from 'mist'\nconst other = state('')\n---\n<span>{other.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let err = mistc::compile_project_dir(&dir).unwrap_err();
    assert!(err.contains("M1025"), "err: {}", err);
    assert!(err.contains("const id = state("), "err: {}", err);
}

#[test]
fn route_param_page_rejected_at_pages_top_level() {
    let dir = std::env::temp_dir().join("mist-route-param-top-level");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    let page = "---\nimport { state } from 'mist'\nconst id = state('')\n---\n<span>{id.value}</span>\n";
    std::fs::write(dir.join("pages/index.mist"), page).unwrap();
    std::fs::write(dir.join("pages/[id].mist"), page).unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let err = mistc::compile_project_dir(&dir).unwrap_err();
    assert!(err.contains("must live in a page directory"), "err: {}", err);
}

#[test]
fn route_param_page_collides_with_flat_page() {
    let dir = std::env::temp_dir().join("mist-route-param-collision");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages/item")).unwrap();
    let page = "---\nimport { state } from 'mist'\nconst id = state('')\n---\n<span>{id.value}</span>\n";
    std::fs::write(dir.join("pages/index.mist"), page).unwrap();
    std::fs::write(dir.join("pages/item.mist"), page).unwrap();
    std::fs::write(dir.join("pages/item/[id].mist"), page).unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let err = mistc::compile_project_dir(&dir).unwrap_err();
    assert!(err.contains("both compile to"), "err: {}", err);
}

#[test]
fn npm_imports_rejected_in_app_mist() {
    let dir = std::env::temp_dir().join("mist-npm-app-reject");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport dayjs from 'dayjs'\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let err = mistc::compile_project_dir(&dir).unwrap_err();
    assert!(err.contains("app.mist") && err.contains("npm imports work in pages"), "err: {}", err);
}

#[test]
fn npm_vendor_require_path_is_depth_aware_for_subpackages() {
    let dir = std::env::temp_dir().join("mist-npm-subpkg");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::create_dir_all(dir.join("packages/shop/pages")).unwrap();
    std::fs::create_dir_all(dir.join("../mist-npm-subpkg-root")).ok();
    std::fs::create_dir_all(dir.join("node_modules/greeting")).unwrap();
    std::fs::write(
        dir.join("node_modules/greeting/package.json"),
        "{ \"name\": \"greeting\", \"version\": \"2.0.0\", \"main\": \"index.js\" }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("node_modules/greeting/index.js"),
        "module.exports = function greet(n) { return 'yo ' + n; };\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("packages/shop/pages/cart.mist"),
        "---\nimport { state } from 'mist'\nimport greet from 'greeting'\nconst msg = state('')\nfunction f() {\n  msg.value = greet('x')\n}\n---\n<span onTap={f}>{msg.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    if std::process::Command::new("npm").arg("--version").output().is_err() {
        eprintln!("skipping: npm not available");
        return;
    }
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    let cart = p
        .files
        .iter()
        .find(|f| f.out_path == "packages/shop/pages/cart/cart")
        .expect("subpackage page missing");
    assert!(
        cart.output.js.contains("require('../../../../vendor/greeting.js')"),
        "js:\n{}",
        cart.output.js
    );
    let vendor = p.files.iter().find(|f| f.out_path == "vendor/greeting").expect("vendor missing");
    assert!(vendor.output.js.contains("yo "), "vendor:\n{}", vendor.output.js);
}

#[test]
fn app_level_min_lib_version_propagates_to_pages_and_inlined_components() {
    let dir = std::env::temp_dir().join("mist-minlib-app-floor");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::create_dir_all(dir.join("components")).unwrap();
    std::fs::write(
        dir.join("components/Label.mist"),
        "---\nimport { props } from 'mist'\nconst { txt } = props({ txt: '' })\n---\n<text user-select>{txt}</text>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport Label from '../components/Label.mist'\nimport { state } from 'mist'\nconst n = state(0)\nfunction pull() { n.value++ }\n---\n<scroll-view scroll-y refresher-enabled onRefresherRefresh={pull}><Label txt=\"x\" />{n.value}</scroll-view>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nexport const config = { minLibVersion: '2.9.0' }\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    let m1027: Vec<&String> = p.warnings.iter().filter(|w| w.contains("M1027")).collect();
    assert!(
        m1027.iter().any(|w| w.contains("refresher-enabled")),
        "app floor must reach pages: {:?}",
        p.warnings
    );
    assert!(
        m1027.iter().any(|w| w.contains("user-select") && w.contains("Label.mist")),
        "app floor must reach inlined components: {:?}",
        p.warnings
    );
}

#[test]
fn page_min_lib_version_overrides_app_floor() {
    let dir = std::env::temp_dir().join("mist-minlib-page-override");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\nexport const config = { minLibVersion: '2.10.1' }\nfunction pull() { n.value++ }\n---\n<scroll-view scroll-y refresher-enabled onRefresherRefresh={pull}>{n.value}</scroll-view>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nexport const config = { minLibVersion: '2.9.0' }\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    assert!(
        p.warnings.iter().all(|w| !w.contains("M1027")),
        "page floor 2.10.1 must override the app's 2.9.0: {:?}",
        p.warnings
    );
}

#[test]
fn m1028_flags_dom_packages_and_trusted_packages_suppresses() {
    if std::process::Command::new("npm").arg("--version").output().is_err() {
        eprintln!("skipping: npm not available");
        return;
    }
    let dir = std::env::temp_dir().join("mist-npm-dom-scan");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::create_dir_all(dir.join("stores")).unwrap();
    std::fs::create_dir_all(dir.join("node_modules/domish")).unwrap();
    std::fs::write(
        dir.join("node_modules/domish/package.json"),
        "{ \"name\": \"domish\", \"version\": \"1.0.0\", \"main\": \"index.js\" }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("node_modules/domish/index.js"),
        "module.exports = function width() { return window.innerWidth || document.body.clientWidth; };\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("stores/size.ts"),
        "import { store } from 'mist'\nimport width from 'domish'\nexport const size = store({ w: 0 })\nexport function measure() { size.value.w = width() }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { size, measure } from '../stores/size.ts'\n---\n<span onTap={measure}>{size.value.w}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    assert!(
        p.warnings.iter().any(|w| w.contains("M1028") && w.contains("domish") && w.contains("window")),
        "warnings: {:?}",
        p.warnings
    );
    let store = p.files.iter().find(|f| f.out_path == "stores/size").expect("store missing");
    assert!(
        store.output.js.contains("require('../vendor/domish.js')"),
        "store js:\n{}",
        store.output.js
    );
    assert!(p.files.iter().any(|f| f.out_path == "vendor/domish"), "vendor bundle missing");

    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nexport const config = { trustedPackages: ['domish'] }\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    let p = mistc::compile_project_dir(&dir).expect("compile failed");
    assert!(
        p.warnings.iter().all(|w| !w.contains("M1028")),
        "trustedPackages must suppress: {:?}",
        p.warnings
    );
}
