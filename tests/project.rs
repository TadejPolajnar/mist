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
