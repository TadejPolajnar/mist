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
    assert!(json.contains("\"styleIsolation\": \"isolated\""), "json:\n{}", json);
}

#[test]
fn component_without_config_gets_default_style_isolation() {
    let src = "---\nimport { props } from 'mist'\nconst { label } = props()\n---\n<span>{label}</span>\n";
    let unit = mistc::compile_unit(src, false).expect("compile failed");
    let json = unit.output.json.expect("component json missing");
    assert!(json.contains("\"styleIsolation\": \"isolated\""), "json:\n{}", json);
}

#[test]
fn component_style_isolation_override_replaces_default() {
    let src = "---\nexport const config = { styleIsolation: 'apply-shared' }\n---\n<span>hi</span>\n";
    let unit = mistc::compile_unit(src, false).expect("compile failed");
    let json = unit.output.json.expect("component json missing");
    assert_eq!(json.matches("styleIsolation").count(), 1, "json:\n{}", json);
    assert!(json.contains("\"styleIsolation\": \"apply-shared\""), "json:\n{}", json);
    assert!(!json.contains("\"isolated\""), "json:\n{}", json);
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
    assert!(!json.contains("styleIsolation"), "json:\n{}", json);
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

#[test]
fn props_work_in_deriveds_and_methods() {
    let src = "---\nimport { state, derived, props } from 'mist'\nconst { count } = props({ count: 0 })\nconst bump = state(0)\nconst label = derived(() => `n=${count} b=${bump.value}`)\nfunction tick(count) {\n  bump.value = bump.value + count\n}\nfunction tock() {\n  bump.value = bump.value + count\n  return { count }\n}\n---\n<span onTap={tock}>{label.value}</span>\n";
    let unit = mistc::compile_unit(src, false).expect("compile failed");
    let js = &unit.output.js;
    assert!(js.contains("`n=${this.data.count} b=${this._bump}`"), "js:\n{}", js);
    assert!(js.contains(", ['bump', 'count']);"), "js:\n{}", js);
    assert!(js.contains("this._bump + count"), "shadowed param must stay bare:\n{}", js);
    assert!(js.contains("this._bump + this.data.count"), "js:\n{}", js);
    assert!(js.contains("{ count: this.data.count }"), "shorthand must expand:\n{}", js);
    assert!(js.contains("observer() { rt.touch(this, 'count'); }"), "js:\n{}", js);
}

#[test]
fn prop_observer_omitted_without_deriveds() {
    let src = "---\nimport { props } from 'mist'\nconst { label } = props()\n---\n<span>{label}</span>\n";
    let unit = mistc::compile_unit(src, false).expect("compile failed");
    assert!(!unit.output.js.contains("observer"), "js:\n{}", unit.output.js);
}

#[test]
fn typed_props_map_to_wechat_property_types() {
    let src = "---\nimport { props } from 'mist'\ninterface Todo { id: number; title: string }\nconst { todo, count, label, done, tags, meta, size, mixed } = props<{ todo: Todo; count: number; label: string; done: boolean; tags: string[]; meta: { a: number }; size: 'sm' | 'lg'; mixed: string | number }>()\n---\n<span>{label}</span>\n";
    let unit = mistc::compile_unit(src, false).expect("compile failed");
    let js = &unit.output.js;
    assert!(js.contains("todo: { type: Object }"), "js:\n{}", js);
    assert!(js.contains("count: { type: Number }"), "js:\n{}", js);
    assert!(js.contains("label: { type: String }"), "js:\n{}", js);
    assert!(js.contains("done: { type: Boolean }"), "js:\n{}", js);
    assert!(js.contains("tags: { type: Array }"), "js:\n{}", js);
    assert!(js.contains("meta: { type: Object }"), "js:\n{}", js);
    assert!(js.contains("size: { type: String }"), "js:\n{}", js);
    assert!(js.contains("mixed: { type: null }"), "js:\n{}", js);
}

#[test]
fn untyped_props_stay_null() {
    let src = "---\nimport { props } from 'mist'\nconst { todo, count, label } = props({ count: 0 })\n---\n<span>{label}</span>\n";
    let unit = mistc::compile_unit(src, false).expect("compile failed");
    let js = &unit.output.js;
    assert!(js.contains("todo: { type: null }"), "js:\n{}", js);
    assert!(js.contains("count: { type: null, value: 0 }"), "js:\n{}", js);
    assert!(js.contains("label: { type: null }"), "js:\n{}", js);
}

#[test]
fn typed_props_mismatched_type_and_default_members_do_not_panic() {
    let src = "---\nimport { props } from 'mist'\nconst { a, b } = props<{ a: number }>({ a: 1, b: 2 })\n---\n<span>{a}</span>\n";
    let unit = mistc::compile_unit(src, false).expect("compile failed");
    let js = &unit.output.js;
    assert!(js.contains("a: { type: Number, value: 1 }"), "js:\n{}", js);
    assert!(js.contains("b: { type: null, value: 2 }"), "js:\n{}", js);
}

#[test]
fn typed_props_property_name_set_is_unchanged_by_typing() {
    let untyped_src = "---\nimport { props } from 'mist'\nconst { a, b, c } = props({ a: 1 })\n---\n<span>{b}</span>\n";
    let typed_src = "---\nimport { props } from 'mist'\nconst { a, b, c } = props<{ a: number; b: string; c: boolean }>({ a: 1 })\n---\n<span>{b}</span>\n";
    let untyped = mistc::compile_unit(untyped_src, false).expect("compile failed");
    let typed = mistc::compile_unit(typed_src, false).expect("compile failed");
    let extract_names = |js: &str| -> Vec<String> {
        let start = js.find("properties: {").unwrap() + "properties: {".len();
        let end = js[start..].find("},\n  data").unwrap() + start;
        js[start..end]
            .lines()
            .filter_map(|l| l.trim().split(':').next())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    };
    assert_eq!(extract_names(&untyped.output.js), extract_names(&typed.output.js));
    assert!(typed.output.js.contains("a: { type: Number, value: 1 }"), "js:\n{}", typed.output.js);
}

#[test]
fn callback_event_options_emit_third_trigger_event_arg() {
    let src = "---\nimport { props } from 'mist'\nexport const config = { events: { onToggle: { bubbles: true } } }\nconst { onToggle } = props()\n---\n<span onTap={() => onToggle()}>hi</span>\n";
    let unit = mistc::compile_unit(src, false).expect("compile failed");
    let js = &unit.output.js;
    assert!(
        js.contains("this.triggerEvent('toggle', { args }, { bubbles: true })"),
        "js:\n{}",
        js
    );
    let json = unit.output.json.expect("component json missing");
    assert!(!json.contains("\"events\""), "json:\n{}", json);
}

#[test]
fn callback_event_options_unknown_prop_errors() {
    let src = "---\nimport { props } from 'mist'\nexport const config = { events: { onMissing: { bubbles: true } } }\nconst { onToggle } = props()\n---\n<span onTap={() => onToggle()}>hi</span>\n";
    let err = match mistc::compile_unit(src, false) {
        Err(e) => e,
        Ok(_) => panic!("expected error"),
    };
    assert!(err.contains("onMissing"), "err: {}", err);
    assert!(err.contains("onToggle"), "err: {}", err);
}

#[test]
fn callback_without_events_config_has_no_third_arg() {
    let src = include_str!("../examples/app/TodoItem.mist");
    let unit = mistc::compile_unit(src, false).expect("compile failed");
    let js = &unit.output.js;
    assert!(js.contains("this.triggerEvent('toggle', { args });"), "js:\n{}", js);
}

#[test]
fn plugin_components_merge_into_using_components_and_drop_from_json() {
    let src = "---\nimport { state } from 'mist'\nexport const config = { pluginComponents: { calendar: 'plugin://calendar/calendar' } }\nconst n = state(0)\n---\n<calendar />\n<span>{n.value}</span>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    let json = unit.output.json.expect("page json missing");
    assert!(json.contains("\"usingComponents\": { \"calendar\": \"plugin://calendar/calendar\" }"), "json:\n{}", json);
    assert!(!json.contains("pluginComponents"), "json:\n{}", json);
    assert!(
        unit.warnings.iter().all(|w| !w.contains("M1019")),
        "warnings: {:?}",
        unit.warnings
    );
}

#[test]
fn plugin_components_merge_with_mist_component_imports() {
    let src = "---\nimport TodoItem from './TodoItem.mist'\nimport { state } from 'mist'\nexport const config = { pluginComponents: { calendar: 'plugin://calendar/calendar' } }\nconst items = state([{ id: 1 }])\n---\n<calendar />\n<TodoItem todo={items.value[0]} />\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    let json = unit.output.json.expect("page json missing");
    assert!(json.contains("\"calendar\": \"plugin://calendar/calendar\""), "json:\n{}", json);
    assert!(json.contains("\"todo-item\": \"./todo-item\""), "json:\n{}", json);
}

#[test]
fn created_and_moved_lifecycles_emit_and_keep_synthesized_attached() {
    let src = "---\nimport { props, onCreate, onMove } from 'mist'\nconst { label } = props()\nonCreate(() => { console.log('create') })\nonMove(() => { console.log('move') })\n---\n<span>{label}</span>\n";
    let unit = mistc::compile_unit(src, false).expect("compile failed");
    let js = &unit.output.js;
    assert!(js.contains("created() {"), "js:\n{}", js);
    assert!(js.contains("moved() {"), "js:\n{}", js);
    assert!(!js.contains("onCreate"), "mist name must not leak:\n{}", js);
    assert!(!js.contains("onMove"), "mist name must not leak:\n{}", js);
    // no onAttach declared: rt.init is still synthesized into attached
    assert!(js.contains("attached() {\n      rt.init(this);"), "js:\n{}", js);
}

#[test]
fn plugin_components_name_collision_with_mist_import_errors() {
    let src = "---\nimport calendar from './TodoItem.mist'\nimport { state } from 'mist'\nexport const config = { pluginComponents: { calendar: 'plugin://calendar/calendar' } }\nconst n = state(0)\n---\n<calendar />\n<span>{n.value}</span>\n";
    let err = match mistc::compile_unit(src, true) {
        Err(e) => e,
        Ok(_) => panic!("expected error"),
    };
    assert!(err.contains("M1015"), "err: {}", err);
}

#[test]
fn component_options_emit_in_js_and_never_reach_json() {
    let src = "---\nimport { props } from 'mist'\nexport const config = { virtualHost: true, pureDataPattern: '^_', externalClasses: ['x-class'] }\nconst { label } = props()\n---\n<span>{label}</span>\n";
    let unit = mistc::compile_unit(src, false).expect("compile failed");
    let js = &unit.output.js;
    assert!(js.contains("virtualHost: true"), "js:\n{}", js);
    assert!(js.contains("pureDataPattern: /^_/"), "js:\n{}", js);
    assert!(js.contains("externalClasses: [\"x-class\"]"), "js:\n{}", js);

    let json = unit.output.json.expect("component json missing");
    assert!(!json.contains("virtualHost"), "json:\n{}", json);
    assert!(!json.contains("pureDataPattern"), "json:\n{}", json);
    assert!(!json.contains("externalClasses"), "json:\n{}", json);
}

#[test]
fn named_slot_and_virtual_host_merge_into_one_options_object() {
    let src = "---\nimport { props } from 'mist'\nexport const config = { virtualHost: true }\nconst { label } = props()\n---\n<view><slot name=\"header\" /><span>{label}</span></view>\n";
    let unit = mistc::compile_unit(src, false).expect("compile failed");
    let js = &unit.output.js;
    assert!(
        js.contains("options: { multipleSlots: true, virtualHost: true },"),
        "js:\n{}",
        js
    );
    assert_eq!(js.matches("options:").count(), 1, "js:\n{}", js);
}

#[test]
fn component_options_on_page_errors() {
    let src = "---\nexport const config = { virtualHost: true }\n---\n<span>hi</span>\n";
    let err = match mistc::compile_unit(src, true) {
        Err(e) => e,
        Ok(_) => panic!("expected error"),
    };
    assert!(err.contains("'virtualHost' is component-only config"), "err: {}", err);
}

#[test]
fn pure_data_pattern_containing_slash_errors() {
    let src = "---\nexport const config = { pureDataPattern: '^_/private' }\n---\n<span>hi</span>\n";
    let err = match mistc::compile_unit(src, false) {
        Err(e) => e,
        Ok(_) => panic!("expected error"),
    };
    assert!(err.contains("pureDataPattern"), "err: {}", err);
    assert!(err.contains("/"), "err: {}", err);
}

#[test]
fn pure_data_pattern_containing_backslash_errors() {
    let src = "---\nexport const config = { pureDataPattern: '^_\\\\' }\n---\n<span>hi</span>\n";
    let err = match mistc::compile_unit(src, false) {
        Err(e) => e,
        Ok(_) => panic!("expected error"),
    };
    assert!(err.contains("pureDataPattern"), "err: {}", err);
    assert!(err.contains('\\'), "err: {}", err);
}

#[test]
fn external_classes_bad_entry_errors() {
    let src = "---\nexport const config = { externalClasses: ['bad class!'] }\n---\n<span>hi</span>\n";
    let err = match mistc::compile_unit(src, false) {
        Err(e) => e,
        Ok(_) => panic!("expected error"),
    };
    assert!(err.contains("externalClasses"), "err: {}", err);
}

#[test]
fn wx_behaviors_emit_into_component_and_stay_out_of_json() {
    let src = "---\nimport { props } from 'mist'\nconst { value } = props({ value: '' })\nexport const config = { behaviors: ['wx://form-field'] }\n---\n<span>{value}</span>\n";
    let unit = mistc::compile_unit(src, false).expect("compile failed");
    assert!(unit.output.js.contains("behaviors: ['wx://form-field'],"), "js:\n{}", unit.output.js);
    let json = unit.output.json.as_deref().unwrap_or("");
    assert!(!json.contains("behaviors"), "json: {}", json);
}

#[test]
fn user_behaviors_are_rejected_with_reason() {
    let src = "---\nimport { props } from 'mist'\nconst { value } = props({ value: '' })\nexport const config = { behaviors: ['my-behavior'] }\n---\n<span>{value}</span>\n";
    let err = match mistc::compile_unit(src, false) {
        Err(e) => e,
        Ok(_) => panic!("user behaviors must be rejected"),
    };
    assert!(err.contains("wx://") && err.contains("my-behavior"), "err: {}", err);
}

#[test]
fn behaviors_rejected_on_pages() {
    let src = "---\nimport { state } from 'mist'\nconst n = state(0)\nexport const config = { behaviors: ['wx://form-field'] }\n---\n<span>{n.value}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("component-only"), "err: {}", err);
}
