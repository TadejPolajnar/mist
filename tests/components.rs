use std::path::Path;

#[test]
fn component_unit_emits_properties_and_events() {
    let src = include_str!("../examples/app/TodoItem.mist");
    let unit = mistc::compile_unit(src, false).expect("compile failed");
    let js = &unit.output.js;

    assert!(js.contains("Component({"), "js:\n{}", js);
    assert!(js.contains("todo: { type: null, value: null }"), "js:\n{}", js);
    // callback prop becomes a triggerEvent method
    assert!(js.contains("onToggle(...args)"), "js:\n{}", js);
    assert!(js.contains("this.triggerEvent('toggle', { args })"), "js:\n{}", js);
    // methods live inside a methods block; attached runs init
    assert!(js.contains("methods: {"), "js:\n{}", js);
    assert!(js.contains("attached() {\n      rt.init(this);"), "js:\n{}", js);
    // template handler routes through the callback prop method
    assert!(unit.output.wxml.contains("bindtap=\"_e0\""), "wxml:\n{}", unit.output.wxml);
    assert!(unit.output.wxml.contains("data-a0=\"{{todo.id}}\""), "wxml:\n{}", unit.output.wxml);
    assert!(js.contains("this.onToggle(e.currentTarget.dataset.a0)"), "js:\n{}", js);
    // component json
    let json = unit.output.json.expect("component json missing");
    assert!(json.contains("\"component\": true"), "json:\n{}", json);
}

#[test]
fn page_using_component_emits_binding_and_using_components() {
    let src = include_str!("../examples/app/index.mist");
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    let wxml = &unit.output.wxml;

    assert!(wxml.contains("<todo-item"), "wxml:\n{}", wxml);
    assert!(wxml.contains("todo=\"{{t}}\""), "wxml:\n{}", wxml);
    assert!(wxml.contains("bind:toggle=\"_e0\""), "wxml:\n{}", wxml);
    assert!(wxml.contains("wx:key=\"id\""), "wxml:\n{}", wxml);

    // parent wrapper unwraps child-sent detail args
    assert!(unit.output.js.contains("const a = (e.detail && e.detail.args) || [];"), "js:\n{}", unit.output.js);
    assert!(unit.output.js.contains("this.toggle(...a);"), "js:\n{}", unit.output.js);

    let json = unit.output.json.expect("page json missing");
    assert!(json.contains("\"todo-item\": \"./todo-item\""), "json:\n{}", json);
    assert!(json.contains("\"navigationBarTitleText\": \"Todos\""), "json:\n{}", json);
}

#[test]
fn compile_project_emits_page_and_component() {
    let project = mistc::compile_project(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/app/index.mist"
    )))
    .expect("project compile failed");
    let files = &project.files;

    let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"index"), "names: {:?}", names);
    assert!(names.contains(&"todo-item"), "names: {:?}", names);

    let page = files.iter().find(|f| f.name == "index").unwrap();
    assert!(page.is_page);
    let comp = files.iter().find(|f| f.name == "todo-item").unwrap();
    assert!(!comp.is_page);
}

#[test]
fn unused_import_is_not_compiled_or_registered() {
    let src = "---\nimport TodoItem from './TodoItem.mist'\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert!(unit.used_imports.is_empty());
    assert!(unit.output.json.is_none(), "json: {:?}", unit.output.json);
}
