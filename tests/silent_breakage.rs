//! `#[ignore]`d tests specify a fix that is not written yet; the paired
//! `*_current_behaviour` test pins today's broken output so the fix cannot
//! land unnoticed.

#[test]
fn state_write_in_method_shorthand_callback_is_m1030() {
    let src = "---\nimport { state } from 'mist'\nconst items = state([])\nfunction load() { wx.request({ url: 'u', success(res) { items.value = res.data } }) }\n---\n<button onTap={load}>{items.value.length}</button>\n";
    let err = match mistc::compile_unit(src, true) {
        Err(e) => e,
        Ok(_) => panic!("expected a compile error"),
    };
    assert!(err.contains("M1030"), "err: {}", err);
    assert!(err.contains("arrow function"), "err: {}", err);
}

#[test]
fn state_write_in_nested_function_declaration_is_m1030() {
    let src = "---\nimport { state } from 'mist'\nconst n = state(0)\nfunction outer() { function inner() { n.value++ } inner() }\n---\n<span onTap={outer}>{n.value}</span>\n";
    let err = match mistc::compile_unit(src, true) {
        Err(e) => e,
        Ok(_) => panic!("expected a compile error"),
    };
    assert!(err.contains("M1030"), "err: {}", err);
}

#[test]
fn arrow_callback_keeps_page_this() {
    let src = "---\nimport { state } from 'mist'\nconst items = state([])\nfunction load() { wx.request({ url: 'u', success: (res) => { items.value = res.data } }) }\n---\n<button onTap={load}>{items.value.length}</button>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert!(
        unit.output.js.contains("success: (res) => { this.__set('items', res.data) }"),
        "js:\n{}",
        unit.output.js
    );
    assert!(unit.warnings.is_empty(), "warnings: {:?}", unit.warnings);
}

#[test]
fn const_arrow_handler_becomes_a_page_method() {
    let src = "---\nimport { state } from 'mist'\nconst n = state(0)\nconst bump = () => { n.value++ }\n---\n<button onTap={bump}>{n.value}</button>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert!(
        unit.output.wxml.contains("bindtap=\"bump\""),
        "wxml:\n{}",
        unit.output.wxml
    );
    assert!(
        unit.output.js.contains("bump() {") || unit.output.js.contains("bump:"),
        "handler bound in wxml but no `bump` method on the page.\njs:\n{}",
        unit.output.js
    );
    assert!(
        unit.output.js.contains("this.__set('n'"),
        "handler body was never rewritten to a reactive write.\njs:\n{}",
        unit.output.js
    );
}

#[test]
fn function_declaration_handler_becomes_a_page_method() {
    let src = "---\nimport { state } from 'mist'\nconst n = state(0)\nfunction bump() { n.value++ }\n---\n<button onTap={bump}>{n.value}</button>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert!(unit.output.wxml.contains("bindtap=\"bump\""), "wxml:\n{}", unit.output.wxml);
    assert!(unit.output.js.contains("bump() {"), "js:\n{}", unit.output.js);
    assert!(unit.output.js.contains("this.__set('n'"), "js:\n{}", unit.output.js);
}

#[test]
fn literal_brace_in_text_is_preserved() {
    let src = "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<view><text>fn(v) { return v }</text><span>{n.value}</span></view>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert!(
        !unit.output.wxml.contains("{{return v}}"),
        "literal text was turned into a data binding.\nwxml:\n{}",
        unit.output.wxml
    );
    assert!(
        unit.output.wxml.contains("fn(v) { return v }"),
        "wxml:\n{}",
        unit.output.wxml
    );
}
