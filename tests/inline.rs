use std::path::Path;

fn project() -> mistc::Project {
    mistc::compile_project(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/app/index.mist")))
        .expect("project compile failed")
}

#[test]
fn pure_render_component_is_inlined_as_template() {
    let p = project();
    let badge = p.files.iter().find(|f| f.name == "badge").expect("badge not compiled");
    assert!(badge.output.wxml.starts_with("<template name=\"badge\">"), "wxml:\n{}", badge.output.wxml);
    assert!(badge.output.js.is_empty(), "inlined template must have no JS:\n{}", badge.output.js);
    assert!(badge.output.json.is_none());
    // prop binding survives inside the template
    assert!(badge.output.wxml.contains("{{count}}"), "wxml:\n{}", badge.output.wxml);
}

#[test]
fn parent_uses_template_import_and_is() {
    let p = project();
    let index = p.files.iter().find(|f| f.name == "index").unwrap();
    assert!(index.output.wxml.contains("<import src=\"./badge.wxml\" />"), "wxml:\n{}", index.output.wxml);
    assert!(
        index.output.wxml.contains("<template is=\"badge\" data=\"{{ count: visible.length }}\" />"),
        "wxml:\n{}",
        index.output.wxml
    );
    // inlined component must NOT be registered in usingComponents
    let json = index.output.json.as_deref().unwrap();
    assert!(!json.contains("badge"), "json:\n{}", json);
    // stateful component still is
    assert!(json.contains("\"todo-item\": \"./todo-item\""), "json:\n{}", json);
}

#[test]
fn stateful_component_is_not_inlined() {
    let p = project();
    let item = p.files.iter().find(|f| f.name == "todo-item").unwrap();
    assert!(item.output.js.contains("Component({"), "js:\n{}", item.output.js);
}

#[test]
fn inlined_component_classes_reach_shared_css() {
    let p = project();
    assert!(p.tailwind_css.contains(".text-sm"), "css:\n{}", p.tailwind_css);
    assert!(p.tailwind_css.contains(".px-2"), "css:\n{}", p.tailwind_css);
}

#[test]
fn slots_pass_through_and_enable_multiple_slots() {
    let child = "---\nimport { props } from 'mist'\nconst { title } = props()\n---\n<div class=\"card\"><slot name=\"header\" /><span>{title}</span><slot /></div>\n";
    let unit = mistc::compile_unit(child, false).expect("compile failed");
    assert!(unit.output.wxml.contains("<slot name=\"header\" />"), "wxml:\n{}", unit.output.wxml);
    assert!(unit.output.wxml.contains("<slot />"), "wxml:\n{}", unit.output.wxml);
    assert!(unit.output.js.contains("options: { multipleSlots: true }"), "js:\n{}", unit.output.js);
}

#[test]
fn parent_children_render_inside_component_tag() {
    let parent = "---\nimport Card from './Card.mist'\nimport { state } from 'mist'\nconst n = state(0)\n---\n<Card title={n.value}><span slot=\"header\">Hi</span><span>{n.value}</span></Card>\n";
    let unit = mistc::compile_unit(parent, true).expect("compile failed");
    let wxml = &unit.output.wxml;
    assert!(wxml.contains("<card title=\"{{n}}\">"), "wxml:\n{}", wxml);
    assert!(wxml.contains("slot=\"header\""), "wxml:\n{}", wxml);
    assert!(wxml.contains("</card>"), "wxml:\n{}", wxml);
}

#[test]
fn inlined_component_rejects_children_and_callbacks() {
    // compile_unit with explicit inline is not public; exercise via a project-shaped error:
    // a component that qualifies for inlining but is passed a callback prop must error.
    let dir = std::env::temp_dir().join("mist-inline-err");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Pure.mist"), "---\nimport { props } from 'mist'\nconst { x } = props()\n---\n<span>{x}</span>\n").unwrap();
    std::fs::write(
        dir.join("page.mist"),
        "---\nimport Pure from './Pure.mist'\nimport { state } from 'mist'\nconst n = state(0)\nfunction f() { n.value++ }\n---\n<Pure x={n.value} onPing={f} />\n",
    )
    .unwrap();
    let err = mistc::compile_project(&dir.join("page.mist")).unwrap_err();
    assert!(err.contains("callback prop"), "err: {}", err);
}

#[test]
fn config_inline_false_opts_out_of_inlining() {
    let dir = std::env::temp_dir().join("mist-inline-optout");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("Chip.mist"),
        "---\nimport { props } from 'mist'\nconst { label } = props()\nexport const config = { inline: false }\n---\n<span>{label}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("page.mist"),
        "---\nimport Chip from './Chip.mist'\nimport { state } from 'mist'\nconst n = state(0)\n---\n<div><Chip label=\"x\" /><span>{n.value}</span></div>\n",
    )
    .unwrap();
    let p = mistc::compile_project(&dir.join("page.mist")).expect("project compile failed");
    let chip = p.files.iter().find(|f| f.name == "chip").expect("chip not compiled");
    assert!(!chip.output.js.is_empty(), "chip must be a real Component, not an inlined template");
    assert!(chip.output.js.contains("Component({"), "js:\n{}", chip.output.js);
    let json = chip.output.json.as_deref().unwrap_or("");
    assert!(!json.contains("inline"), "json leaked inline key:\n{}", json);
}

#[test]
fn component_with_plain_const_is_not_inlined() {
    let dir = std::env::temp_dir().join("mist-inline-const");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("Badge.mist"),
        "---\nimport { props } from 'mist'\nconst { count } = props({ count: 0 })\nconst LABEL = 'items'\n---\n<span>{count} {LABEL}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("page.mist"),
        "---\nimport Badge from './Badge.mist'\nimport { state } from 'mist'\nconst n = state(0)\n---\n<div><Badge count={n.value} /><span>{n.value}</span></div>\n",
    )
    .unwrap();
    let p = mistc::compile_project(&dir.join("page.mist")).expect("project compile failed");
    let badge = p.files.iter().find(|f| f.name == "badge").expect("badge not compiled");
    assert!(badge.output.js.contains("Component({"), "const-bearing component must stay real:\n{}", badge.output.js);
    assert!(badge.output.js.contains("LABEL: LABEL,"), "const must seed component data:\n{}", badge.output.js);
}
