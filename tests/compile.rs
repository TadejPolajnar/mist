fn page(frontmatter: &str, template: &str) -> String {
    format!("---\n{}\n---\n{}\n", frontmatter, template)
}

fn compile(frontmatter: &str, template: &str) -> mistc::Output {
    mistc::compile(&page(frontmatter, template)).expect("compile failed")
}

// ---- reactivity: mutation compilation ----

#[test]
fn update_expression_compiles_to_set() {
    let out = compile(
        "import { state } from 'mist'\nconst count = state(0)\nfunction inc() { count.value++ }",
        "<button onTap={inc}>{count.value}</button>",
    );
    assert!(out.js.contains("this.__set('count', this.data.count + 1)"), "js:\n{}", out.js);
}

#[test]
fn decrement_compiles_to_set() {
    let out = compile(
        "import { state } from 'mist'\nconst count = state(0)\nfunction dec() { count.value-- }",
        "<button onTap={dec}>{count.value}</button>",
    );
    assert!(out.js.contains("this.__set('count', this.data.count - 1)"), "js:\n{}", out.js);
}

#[test]
fn compound_assignment_reads_old_value() {
    let out = compile(
        "import { state } from 'mist'\nconst count = state(0)\nfunction add(n) { count.value += n }",
        "<button onTap={() => add(5)}>{count.value}</button>",
    );
    assert!(out.js.contains("this.__set('count', this.data.count + (n))"), "js:\n{}", out.js);
}

#[test]
fn nested_object_path() {
    let out = compile(
        "import { state } from 'mist'\nconst user = state({ name: '', vip: false })\nfunction rename() { user.value.name = 'Ada' }",
        "<span>{user.value.name}</span>",
    );
    assert!(out.js.contains("this.__set(`user.name`, 'Ada')"), "js:\n{}", out.js);
}

#[test]
fn dynamic_index_path_uses_template_literal() {
    let out = compile(
        "import { state } from 'mist'\nconst items = state([])\nfunction set(i, v) { items.value[i].label = v }",
        "<span>{items.value.length}</span>",
    );
    assert!(out.js.contains("this.__set(`items[${i}].label`, v)"), "js:\n{}", out.js);
}

#[test]
fn push_compiles_to_length_indexed_set() {
    let out = compile(
        "import { state } from 'mist'\nconst items = state([])\nfunction add(x) { items.value.push(x) }",
        "<span>{items.value.length}</span>",
    );
    assert!(
        out.js.contains("this.__set(`items[${this.data.items.length}]`, x)"),
        "js:\n{}",
        out.js
    );
}

#[test]
fn splice_is_rejected_with_diagnostic() {
    let err = mistc::compile(&page(
        "import { state } from 'mist'\nconst items = state([])\nfunction rm(i) { items.value.splice(i, 1) }",
        "<span>{items.value.length}</span>",
    ))
    .unwrap_err();
    assert!(err.contains("M1004"), "err: {}", err);
}

#[test]
fn sort_is_rejected_with_diagnostic() {
    let err = mistc::compile(&page(
        "import { state } from 'mist'\nconst items = state([])\nfunction s() { items.value.sort() }",
        "<span>{items.value.length}</span>",
    ))
    .unwrap_err();
    assert!(err.contains("M1004"), "err: {}", err);
}

#[test]
fn full_reassignment_is_single_key_write() {
    let out = compile(
        "import { state } from 'mist'\nconst items = state([1])\nfunction reset() { items.value = [] }",
        "<span>{items.value.length}</span>",
    );
    assert!(out.js.contains("this.__set('items', [])"), "js:\n{}", out.js);
}

#[test]
fn non_state_assignments_untouched() {
    let out = compile(
        "import { state } from 'mist'\nconst count = state(0)\nfunction f() { let x = 1; x += 2 }",
        "<span>{count.value}</span>",
    );
    assert!(out.js.contains("x += 2"), "js:\n{}", out.js);
}

// ---- derived ----

#[test]
fn derived_reads_rewritten_and_initialized() {
    let out = compile(
        "import { state, derived } from 'mist'\nconst n = state(2)\nconst double = derived(() => n.value * 2)",
        "<span>{double.value}</span>",
    );
    assert!(out.js.contains("rt.derive(this, __o, 'double', null, () => this._n * 2)"), "js:\n{}", out.js);
    assert!(out.js.contains("double: null"), "js:\n{}", out.js);
    assert!(out.wxml.contains("{{double}}"), "wxml:\n{}", out.wxml);
}

#[test]
fn no_derived_emits_empty_derive() {
    let out = compile(
        "import { state } from 'mist'\nconst n = state(1)",
        "<span>{n.value}</span>",
    );
    assert!(out.js.contains("__derive() {\n    const __o = {};\n    return __o;\n  }"), "js:\n{}", out.js);
}

// ---- methods calling methods, lifecycles ----

#[test]
fn method_calls_rewritten_to_this() {
    let out = compile(
        "import { state } from 'mist'\nconst n = state(0)\nfunction inc() { n.value++ }\nfunction twice() { inc()\n inc() }",
        "<button onTap={twice}>x</button>",
    );
    assert!(out.js.contains("this.inc()"), "js:\n{}", out.js);
}

#[test]
fn user_onload_merged_with_derive() {
    let out = compile(
        "import { state, onLoad } from 'mist'\nconst id = state(null)\nonLoad((query) => {\n  id.value = query.id\n})",
        "<span>{id.value}</span>",
    );
    assert!(out.js.contains("onLoad(query)"), "js:\n{}", out.js);
    assert!(out.js.contains("rt.init(this);"), "js:\n{}", out.js);
    assert!(out.js.contains("this.__set('id', query.id)"), "js:\n{}", out.js);
    // exactly one onLoad
    assert_eq!(out.js.matches("onLoad").count(), 1, "js:\n{}", out.js);
}

#[test]
fn async_lifecycle_and_method() {
    let out = compile(
        "import { state, onLoad } from 'mist'\nconst d = state(null)\nasync function refresh() { d.value = await fetchData() }\nonLoad(async () => {\n  await refresh()\n})",
        "<span>{d.value}</span>",
    );
    assert!(out.js.contains("async refresh()"), "js:\n{}", out.js);
    assert!(out.js.contains("async onLoad()"), "js:\n{}", out.js);
    assert!(out.js.contains("await this.refresh()"), "js:\n{}", out.js);
}

#[test]
fn state_mutation_inside_lifecycle_compiled() {
    let out = compile(
        "import { state, onShow } from 'mist'\nconst n = state(0)\nonShow(() => {\n  n.value = 1\n})",
        "<span>{n.value}</span>",
    );
    assert!(out.js.contains("onShow()"), "js:\n{}", out.js);
    assert!(out.js.contains("this.__set('n', 1)"), "js:\n{}", out.js);
}

// ---- template ----

#[test]
fn ternary_in_binding_passes_through() {
    let out = compile(
        "import { state } from 'mist'\nconst on = state(false)",
        "<span class={on.value ? 'a' : 'b'}>x</span>",
    );
    assert!(out.wxml.contains("class=\"{{on ? 'a' : 'b'}}\""), "wxml:\n{}", out.wxml);
}

#[test]
fn logical_and_without_jsx_stays_binding() {
    let out = compile(
        "import { state } from 'mist'\nconst a = state(1)\nconst b = state(2)",
        "<span>{a.value && b.value}</span>",
    );
    assert!(out.wxml.contains("{{a && b}}"), "wxml:\n{}", out.wxml);
    assert!(!out.wxml.contains("wx:if"), "wxml:\n{}", out.wxml);
}

#[test]
fn key_star_this_for_primitive_lists() {
    let out = compile(
        "import { state } from 'mist'\nconst tags = state(['a'])",
        "<div>{tags.value.map(tag => (<span key={tag}>{tag}</span>))}</div>",
    );
    assert!(out.wxml.contains("wx:key=\"*this\""), "wxml:\n{}", out.wxml);
}

#[test]
fn key_index_emits_no_wx_key() {
    let out = compile(
        "import { state } from 'mist'\nconst tags = state(['a'])",
        "<div>{tags.value.map(tag => (<span key={index}>{tag}</span>))}</div>",
    );
    assert!(!out.wxml.contains("wx:key"), "wxml:\n{}", out.wxml);
}

#[test]
fn event_modifiers() {
    let out = compile(
        "import { state } from 'mist'\nconst n = state(0)\nfunction f() { n.value++ }",
        "<div onTap:catch={f}><button onClick={f}>x</button></div>",
    );
    assert!(out.wxml.contains("catchtap=\"f\""), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("bindtap=\"f\""), "wxml:\n{}", out.wxml);
}

#[test]
fn multi_arg_inline_handler() {
    let out = compile(
        "import { state } from 'mist'\nconst n = state(0)\nfunction f(a, b) { n.value = a + b }",
        "<button onTap={() => f(1, n.value)}>x</button>",
    );
    assert!(out.wxml.contains("data-a0=\"{{1}}\""), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("data-a1=\"{{n}}\""), "wxml:\n{}", out.wxml);
    assert!(
        out.js.contains("this.f(e.currentTarget.dataset.a0, e.currentTarget.dataset.a1)"),
        "js:\n{}",
        out.js
    );
}

#[test]
fn self_closing_and_tag_mapping() {
    let out = compile(
        "import { state } from 'mist'\nconst src = state('x.png')",
        "<div><img src={src.value} mode=\"aspectFill\" /><p>hi</p></div>",
    );
    assert!(out.wxml.contains("<image src=\"{{src}}\" mode=\"aspectFill\" />"), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("<view>hi</view>"), "wxml:\n{}", out.wxml);
}

#[test]
fn nested_loops() {
    let out = compile(
        "import { state } from 'mist'\nconst groups = state([])",
        "<div>{groups.value.map(g => (<div key={g.id}>{g.items.map(it => (<span key={it.id}>{it.name}</span>))}</div>))}</div>",
    );
    assert!(out.wxml.contains("wx:for=\"{{groups}}\""), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("wx:for-item=\"g\""), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("wx:for=\"{{g.items}}\""), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("wx:for-item=\"it\""), "wxml:\n{}", out.wxml);
}

// ---- sfc / config ----

#[test]
fn missing_frontmatter_errors() {
    assert!(mistc::compile("<div>x</div>").is_err());
}

#[test]
fn unclosed_tag_errors() {
    assert!(mistc::compile(&page("import { state } from 'mist'\nconst n = state(0)", "<div><span>x</div>")).is_err());
}

#[test]
fn config_object_to_json() {
    let out = compile(
        "import { state } from 'mist'\nexport const config = { navigationBarTitleText: 'Hi', enablePullDownRefresh: true }\nconst n = state(0)",
        "<span>{n.value}</span>",
    );
    let json: serde_free_check::Value = serde_free_check::parse(out.json.as_deref().expect("no json"));
    assert!(json.ok, "json not parseable: {:?}", out.json);
}

// minimal JSON well-formedness check without pulling serde in
mod serde_free_check {
    pub struct Value {
        pub ok: bool,
    }
    pub fn parse(s: &str) -> Value {
        let mut depth = 0i32;
        let mut in_str = false;
        let mut prev = ' ';
        for c in s.chars() {
            if in_str {
                if c == '"' && prev != '\\' {
                    in_str = false;
                }
            } else {
                match c {
                    '"' => in_str = true,
                    '{' | '[' => depth += 1,
                    '}' | ']' => depth -= 1,
                    '\'' => return Value { ok: false },
                    _ => {}
                }
            }
            prev = c;
        }
        Value { ok: depth == 0 && !in_str && s.trim_start().starts_with('{') }
    }
}

#[test]
fn style_block_emitted_to_wxss() {
    let src = "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n<style>\n.a { color: red; }\n</style>\n".to_string();
    let out = mistc::compile(&src).expect("compile failed");
    assert!(out.wxss.contains(".a { color: red; }"), "wxss:\n{}", out.wxss);
}

#[test]
fn expression_body_lifecycle_arrows_emit_valid_js() {
    let out = compile(
        "import { state, onShow } from 'mist'\nconst n = state(0)\nonShow(() => n.value++)",
        "<span>{n.value}</span>",
    );
    assert!(out.js.contains("onShow() { this.__set('n', this.data.n + 1) }"), "js:\n{}", out.js);
}

#[test]
fn value_bind_two_way_input() {
    let out = compile(
        "import { state } from 'mist'\nconst note = state('')",
        "<input value:bind={note} placeholder=\"note\" />",
    );
    assert!(out.wxml.contains("model:value=\"{{note}}\" bindinput=\"__vb_note\""), "wxml:\n{}", out.wxml);
    // bound via value:bind even though no {note.value} read exists
    assert!(out.js.contains("note: ''"), "js:\n{}", out.js);
    assert!(out.js.contains("__vb_note(e) {\n    this.data.note = e.detail.value;\n    rt.touch(this);\n  }"), "js:\n{}", out.js);
}

#[test]
fn mutating_methods_on_copies_are_allowed() {
    let out = compile(
        "import { state, derived } from 'mist'\nconst xs = state([3, 1])\nconst sorted = derived(() => xs.value.slice().sort())",
        "<span>{sorted.value.length}</span>",
    );
    assert!(out.js.contains("this._xs.slice().sort()"), "js:\n{}", out.js);
}

#[test]
fn page_scope_call_expressions_are_hoisted() {
    let out = compile(
        "import { state } from 'mist'\nconst n = state(1500)\nfunction fmt(v) { return '¥' + v.toFixed(2) }",
        "<span>{fmt(n.value)}</span>",
    );
    assert!(out.wxml.contains("{{_h0}}"), "wxml:\n{}", out.wxml);
    assert!(out.js.contains("rt.derive(this, __o, '_h0', null, () => (this.fmt(this.data.n)))"), "js:\n{}", out.js);
    assert!(out.js.contains("_h0: null"), "js:\n{}", out.js);
}

#[test]
fn per_item_call_expressions_become_computed_fields() {
    let out = compile(
        "import { state } from 'mist'\nconst txs = state([{ id: 1, ts: 0 }])\nfunction fmtDate(ts) { return new Date(ts).toDateString() }",
        "<div>{txs.value.map(t => (<span key={t.id}>{fmtDate(t.ts)} {t.id}</span>))}</div>",
    );
    assert!(out.wxml.contains("wx:for=\"{{_hl0}}\""), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("{{t._c0}}"), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("wx:key=\"id\""), "wxml:\n{}", out.wxml);
    assert!(
        out.js.contains("rt.derive(this, __o, '_hl0', 'id', () => (this.data.txs).map(t => ({ ...t, _c0: this.fmtDate(t.ts) })))"),
        "js:\n{}",
        out.js
    );
}
