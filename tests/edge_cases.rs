//! Error paths and edge cases. These compile fine today; the point is that a
//! future refactor shouldn't silently change how they behave — several of these
//! guard messages users are told to rely on in docs/diagnostics.md.

fn page(frontmatter: &str, template: &str) -> String {
    format!("---\n{}\n---\n{}\n", frontmatter, template)
}

fn err(frontmatter: &str, template: &str) -> String {
    mistc::compile(&page(frontmatter, template)).unwrap_err()
}

fn ok(frontmatter: &str, template: &str) -> mistc::Output {
    mistc::compile(&page(frontmatter, template)).expect("expected compile to succeed")
}

const S: &str = "import { state } from 'mist'\nconst n = state(0)";

// ---- template error paths ----

#[test]
fn unquoted_attribute_value_is_rejected() {
    let e = err(S, "<div class=oops>{n.value}</div>");
    assert!(e.contains("malformed attribute value for 'class'"), "err: {}", e);
}

#[test]
fn unknown_event_modifier_is_rejected() {
    let e = err(
        "import { state } from 'mist'\nconst n = state(0)\nfunction f() { n.value++ }",
        "<div onTap:bogus={f}>{n.value}</div>",
    );
    assert!(e.contains("unknown event modifier 'bogus'"), "err: {}", e);
}

#[test]
fn arrow_handler_with_event_param_is_rejected() {
    // documented in docs/language.md: inline arrows may only be `() => method(args)`
    let e = err(
        "import { state } from 'mist'\nconst n = state(0)\nfunction f(e) { n.value++ }",
        "<div onTap={(e) => f(e)}>{n.value}</div>",
    );
    assert!(e.contains("unsupported event expression"), "err: {}", e);
}

#[test]
fn unknown_tag_passes_through_untouched() {
    // native components must not be mangled by tag mapping
    let out = ok(S, "<scroll-view scroll-y><swiper><swiper-item>{n.value}</swiper-item></swiper></scroll-view>");
    assert!(out.wxml.contains("<scroll-view scroll-y>"), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("<swiper-item>"), "wxml:\n{}", out.wxml);
}

// ---- reactivity edge cases ----

#[test]
fn deeply_nested_state_path_compiles_precisely() {
    let out = ok(
        "import { state } from 'mist'\nconst s = state({ a: { b: { c: [1] } } })\nfunction m() { s.value.a.b.c[0] = 9 }",
        "<div onTap={m}>{s.value.a.b.c[0]}</div>",
    );
    assert!(out.js.contains("this.__set(`s.a.b.c[${0}]`, 9)"), "js:\n{}", out.js);
}

#[test]
fn empty_template_compiles() {
    let out = ok(S, "");
    assert!(out.wxml.trim().is_empty(), "wxml:\n{}", out.wxml);
    assert!(out.js.contains("Page({"), "js:\n{}", out.js);
}

#[test]
fn state_with_no_mutations_still_lands_in_data() {
    let out = ok("import { state } from 'mist'\nconst greeting = state('hi')", "<span>{greeting.value}</span>");
    assert!(out.js.contains("greeting: 'hi'"), "js:\n{}", out.js);
}

#[test]
fn multiple_deriveds_chain_in_declaration_order() {
    // a derived reading another derived must be emitted after it, so the
    // runtime's progressive mirror update gives it a fresh value
    let out = ok(
        "import { state, derived } from 'mist'\nconst n = state(2)\nconst dbl = derived(() => n.value * 2)\nconst quad = derived(() => dbl.value * 2)",
        "<span>{quad.value}</span>",
    );
    let dbl_at = out.js.find("'dbl'").expect("dbl missing");
    let quad_at = out.js.find("'quad'").expect("quad missing");
    assert!(dbl_at < quad_at, "derive order wrong:\n{}", out.js);
}

// ---- unicode / text handling ----

#[test]
fn multibyte_text_survives_the_parser() {
    // the ¥ (RMB) sign is 2 bytes; CJK is 3; emoji is 4 — all must round-trip
    let out = ok(S, "<div><span>¥{n.value} 元 🎉</span></div>");
    assert!(out.wxml.contains("¥{{n}} 元 🎉"), "wxml:\n{}", out.wxml);
}

#[test]
fn multibyte_in_class_and_attribute_values() {
    let out = ok(S, "<div class=\"p-4\" data-label=\"金额\">{n.value}</div>");
    assert!(out.wxml.contains("data-label=\"金额\""), "wxml:\n{}", out.wxml);
}

// ---- store edge cases ----

#[test]
fn store_module_rejects_non_mist_imports() {
    let e = mistc::frontmatter::compile_store_module(
        "import { store } from 'mist'\nimport lodash from 'lodash'\nexport const s = store(1)\n",
        "./mist-rt.js",
    )
    .unwrap_err();
    assert!(e.contains("can only import from 'mist'"), "err: {}", e);
}

#[test]
fn store_module_rejects_derived() {
    let e = mistc::frontmatter::compile_store_module(
        "import { store, derived } from 'mist'\nexport const s = store(1)\nexport const d = derived(() => s.value)\n",
        "./mist-rt.js",
    )
    .unwrap_err();
    assert!(e.contains("derived"), "err: {}", e);
}

// ---- config edge cases ----

#[test]
fn config_handles_nested_objects_arrays_and_booleans() {
    let out = ok(
        "import { state } from 'mist'\nconst n = state(0)\nexport const config = { enablePullDownRefresh: true, backgroundTextStyle: 'dark', usingComponents: {}, list: [1, 2, -3] }",
        "<span>{n.value}</span>",
    );
    let json = out.json.expect("json missing");
    assert!(json.contains("\"enablePullDownRefresh\": true"), "json:\n{}", json);
    assert!(json.contains("[1, 2, -3]"), "json:\n{}", json);
}

#[test]
fn config_escapes_quotes_in_strings() {
    let out = ok(
        "import { state } from 'mist'\nconst n = state(0)\nexport const config = { navigationBarTitleText: 'He said \"hi\"' }",
        "<span>{n.value}</span>",
    );
    let json = out.json.expect("json missing");
    assert!(json.contains("\\\"hi\\\""), "json:\n{}", json);
}
