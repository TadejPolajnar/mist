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
    assert!(out.js.contains("rt.derive(this, __o, 'double', null, () => this._n * 2, ['n'])"), "js:\n{}", out.js);
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
fn jsx_ternary_becomes_if_else() {
    let out = compile(
        "import { state } from 'mist'\nconst on = state(false)",
        "<div>{on.value ? (<span>A</span>) : (<span>B</span>)}</div>",
    );
    assert!(out.wxml.contains("<block wx:if=\"{{on}}\">"), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("<block wx:else>"), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("<text>A</text>"), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("<text>B</text>"), "wxml:\n{}", out.wxml);
}

#[test]
fn chained_jsx_ternary_becomes_wx_elif() {
    let out = compile(
        "import { state } from 'mist'\nconst a = state(false)\nconst b = state(false)",
        "<div>{a.value ? <span>X</span> : b.value ? <span>Y</span> : <span>Z</span>}</div>",
    );
    assert!(out.wxml.contains("<block wx:if=\"{{a}}\">"), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("<block wx:elif=\"{{b}}\">"), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("<block wx:else>"), "wxml:\n{}", out.wxml);
    assert!(!out.wxml.contains("<block wx:else>\n  <block wx:if"), "nested if inside else, wxml:\n{}", out.wxml);
}

#[test]
fn non_jsx_ternary_in_text_stays_binding() {
    let out = compile(
        "import { state } from 'mist'\nconst x = state(false)",
        "<span>{x.value ? 'a' : 'b'}</span>",
    );
    assert!(out.wxml.contains("{{x ? 'a' : 'b'}}"), "wxml:\n{}", out.wxml);
    assert!(!out.wxml.contains("wx:if"), "wxml:\n{}", out.wxml);
}

#[test]
fn optional_chaining_and_nullish_before_ternary_splits_correctly() {
    let out = compile(
        "import { state } from 'mist'\nconst a = state(null)\nconst c = state(false)",
        "<div>{a.value?.b ?? c.value ? <span>X</span> : <span>Y</span>}</div>",
    );
    assert!(out.wxml.contains("<block wx:if=\"{{a?.b ?? c}}\">"), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("<block wx:else>"), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("<text>X</text>"), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("<text>Y</text>"), "wxml:\n{}", out.wxml);
}

#[test]
fn ternary_loop_body_child_still_extracts_key() {
    let out = compile(
        "import { state } from 'mist'\nconst items = state([])",
        "<div>{items.value.map(t => (<div>{t.on ? <span key={t.id}>{t.title}</span> : <span>none</span>}</div>))}</div>",
    );
    assert!(out.wxml.contains("wx:key=\"id\""), "wxml:\n{}", out.wxml);
    assert!(!out.wxml.contains("key=\"{{"), "wxml:\n{}", out.wxml);
}

#[test]
fn bare_ternary_map_body_compiles_to_for_and_if() {
    let out = compile(
        "import { state } from 'mist'\nconst items = state([])",
        "<div>{items.value.map(t => t.done ? <span>A</span> : <span>B</span>)}</div>",
    );
    assert!(out.wxml.contains("wx:for="), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("wx:if="), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("wx:else"), "wxml:\n{}", out.wxml);
    assert!(!out.js.contains("<span"), "js:\n{}", out.js);
}

#[test]
fn bare_ternary_map_body_with_key_extracts_key() {
    let out = compile(
        "import { state } from 'mist'\nconst items = state([])",
        "<div>{items.value.map(t => t.done ? <span key={t.id}>A</span> : <span>B</span>)}</div>",
    );
    assert!(out.wxml.contains("wx:key=\"id\""), "wxml:\n{}", out.wxml);
    assert!(!out.wxml.contains("key=\"{{"), "wxml:\n{}", out.wxml);
}

#[test]
fn parenthesized_ternary_map_body_still_compiles() {
    let out = compile(
        "import { state } from 'mist'\nconst items = state([])",
        "<div>{items.value.map(t => (t.done ? <span key={t.id}>A</span> : <span>B</span>))}</div>",
    );
    assert!(out.wxml.contains("wx:for="), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("wx:if="), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("wx:key=\"id\""), "wxml:\n{}", out.wxml);
}

#[test]
fn unparenthesized_jsx_ternary_with_colon_attr_then_branch_splits_correctly() {
    let out = compile(
        "import { state } from 'mist'\nconst on = state(false)\nconst a = state('')",
        "<div>{on.value ? <input value:bind={a} /> : <span>off</span>}</div>",
    );
    assert!(out.wxml.contains("<block wx:if=\"{{on}}\">"), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("model:value=\"{{a}}\" bindinput=\"__vb_a\""), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("<block wx:else>"), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("<text>off</text>"), "wxml:\n{}", out.wxml);
}

#[test]
fn unparenthesized_jsx_ternary_with_colon_attr_on_tap_catch() {
    let out = compile(
        "import { state } from 'mist'\nconst on = state(false)\nfunction f() {}",
        "<div>{on.value ? <div onTap:catch={f}>x</div> : <span/>}</div>",
    );
    assert!(out.wxml.contains("<block wx:if=\"{{on}}\">"), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("catchtap=\"f\""), "wxml:\n{}", out.wxml);
}

#[test]
fn unparenthesized_jsx_ternary_with_nested_element_then_branch() {
    let out = compile(
        "import { state } from 'mist'\nconst on = state(false)",
        "<div>{on.value ? <div><span>a</span></div> : <span/>}</div>",
    );
    assert!(out.wxml.contains("<block wx:if=\"{{on}}\">"), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("<text>a</text>"), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("<block wx:else>"), "wxml:\n{}", out.wxml);
}

#[test]
fn chained_ternary_with_colon_attr_then_branch_emits_wx_elif() {
    let out = compile(
        "import { state } from 'mist'\nconst a = state(false)\nconst b = state(false)\nconst x = state('')",
        "<div>{a.value ? <input value:bind={x} /> : b.value ? <span/> : <div/>}</div>",
    );
    assert!(out.wxml.contains("<block wx:if=\"{{a}}\">"), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("<block wx:elif=\"{{b}}\">"), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("model:value=\"{{x}}\" bindinput=\"__vb_x\""), "wxml:\n{}", out.wxml);
}

#[test]
fn template_referenced_const_is_seeded_into_data() {
    let out = compile(
        "import { state } from 'mist'\nconst n = state(0)\nconst TABS = [{ id: 'a', label: 'A' }, { id: 'b', label: 'B' }]\nconst HIDDEN = [1, 2]\nfunction pick(id) { n.value++ }",
        "<div>{TABS.map(t => (<span key={t.id} onTap={() => pick(t.id)}>{t.label}</span>))}<span>{n.value}</span></div>",
    );
    assert!(out.js.contains("TABS: TABS,"), "const list must seed data:\n{}", out.js);
    assert!(!out.js.contains("HIDDEN: HIDDEN"), "unreferenced const must not seed data:\n{}", out.js);
    assert!(out.wxml.contains("wx:for=\"{{TABS}}\""), "wxml:\n{}", out.wxml);
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

#[test]
fn loop_index_param_emits_for_index() {
    let out = compile(
        "import { state } from 'mist'\nconst todos = state([])",
        "<div>{todos.value.map((t, i) => (<div key={t.id}>{i}: {t.title}</div>))}</div>",
    );
    assert!(out.wxml.contains("wx:for-item=\"t\""), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("wx:for-index=\"i\""), "wxml:\n{}", out.wxml);
    assert!(!out.wxml.contains("wx:for-item=\"t, i\""), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("{{i}}"), "wxml:\n{}", out.wxml);
}

#[test]
fn loop_key_using_index_name_emits_no_wx_key() {
    let out = compile(
        "import { state } from 'mist'\nconst todos = state([])",
        "<div>{todos.value.map((t, i) => (<div key={i}>{t.title}</div>))}</div>",
    );
    assert!(!out.wxml.contains("wx:key"), "wxml:\n{}", out.wxml);
}

#[test]
fn key_inside_conditional_in_loop_body_is_found() {
    let out = compile(
        "import { state } from 'mist'\nconst items = state([])",
        "<div>{items.value.map(t => (<div>{t.on && <span key={t.id}>{t.title}</span>}</div>))}</div>",
    );
    assert!(out.wxml.contains("wx:key=\"id\""), "wxml:\n{}", out.wxml);
    assert!(!out.wxml.contains("key=\"{{"), "wxml:\n{}", out.wxml);
}

#[test]
fn key_on_second_sibling_in_loop_body_is_found() {
    let out = compile(
        "import { state } from 'mist'\nconst items = state([])",
        "<div>{items.value.map(t => (<div><span>{t.title}</span><span key={t.id}>{t.id}</span></div>))}</div>",
    );
    assert!(out.wxml.contains("wx:key=\"id\""), "wxml:\n{}", out.wxml);
    assert!(!out.wxml.contains("key=\"{{"), "wxml:\n{}", out.wxml);
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
    let src = format!(
        "---\nimport {{ state }} from 'mist'\nconst n = state(0)\n---\n<span>{{n.value}}</span>\n<style>\n.a {{ color: red; }}\n</style>\n"
    );
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
    assert!(out.js.contains("__vb_note(e) {\n    this.data.note = e.detail.value;\n    rt.touch(this, 'note');\n  }"), "js:\n{}", out.js);
}

#[test]
fn checked_bind_two_way_switch() {
    let out = compile(
        "import { state } from 'mist'\nconst on = state(false)",
        "<switch checked:bind={on} />",
    );
    assert!(out.wxml.contains("model:checked=\"{{on}}\" bindchange=\"__vb_on\""), "wxml:\n{}", out.wxml);
    assert!(out.js.contains("__vb_on(e) {\n    this.data.on = e.detail.value;\n    rt.touch(this, 'on');\n  }"), "js:\n{}", out.js);
}

#[test]
fn checked_bind_only_state_not_dead_data_eliminated() {
    let out = compile(
        "import { state } from 'mist'\nconst on = state(false)",
        "<switch checked:bind={on} />",
    );
    assert!(out.js.contains("on: false"), "js:\n{}", out.js);
}

#[test]
fn bind_only_state_mutated_in_method_uses_set_not_instance_field() {
    let out = compile(
        "import { state } from 'mist'\nconst on = state(false)\nfunction toggle() { on.value = !on.value }",
        "<switch checked:bind={on} />",
    );
    assert!(out.js.contains("on: false"), "js:\n{}", out.js);
    assert!(out.js.contains("__vb_on(e)"), "js:\n{}", out.js);
    assert!(out.js.contains("toggle() { this.__set('on', !this.data.on) }"), "js:\n{}", out.js);
    assert!(!out.js.contains("this._on"), "js:\n{}", out.js);
}

#[test]
fn unsupported_two_way_bind_prop_errors() {
    let src = page(
        "import { state } from 'mist'\nconst x = state('')",
        "<input foo:bind={x} />",
    );
    let err = mistc::compile(&src).expect_err("expected error for unsupported two-way binding");
    assert!(
        err.contains("unsupported two-way binding 'foo:bind' — supported: value:bind, checked:bind"),
        "err: {}",
        err
    );
}

#[test]
fn same_state_bound_via_value_and_checked_emits_one_handler() {
    let out = compile(
        "import { state } from 'mist'\nconst shared = state('')",
        "<input value:bind={shared} /><switch checked:bind={shared} />",
    );
    assert!(out.wxml.contains("model:value=\"{{shared}}\" bindinput=\"__vb_shared\""), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("model:checked=\"{{shared}}\" bindchange=\"__vb_shared\""), "wxml:\n{}", out.wxml);
    let count = out.js.matches("__vb_shared(e)").count();
    assert_eq!(count, 1, "expected exactly one __vb_shared handler, js:\n{}", out.js);
}

#[test]
fn bind_needle_inside_static_attr_text_does_not_falsely_bind_state() {
    let out = compile(
        "import { state } from 'mist'\nconst ghost = state(false)",
        "<view data-note=\"checked:bind={ghost}\" />",
    );
    assert!(out.js.contains("this._ghost = false"), "js:\n{}", out.js);
    assert!(!out.js.contains("ghost:"), "js:\n{}", out.js);
}

#[test]
fn value_needle_inside_static_attr_text_does_not_falsely_bind_state() {
    let out = compile(
        "import { state } from 'mist'\nconst count = state(0)",
        "<view data-x=\"count.value\" />",
    );
    assert!(out.js.contains("this._count = 0"), "js:\n{}", out.js);
    assert!(!out.js.contains("count:"), "js:\n{}", out.js);
}

#[test]
fn real_expr_value_read_still_marks_state_bound() {
    let out = compile(
        "import { state } from 'mist'\nconst count = state(0)",
        "<text>{count.value}</text>",
    );
    assert!(out.js.contains("count: 0"), "js:\n{}", out.js);
}

#[test]
fn value_read_only_in_for_list_marks_state_bound() {
    let out = compile(
        "import { state } from 'mist'\nconst items = state([1, 2])",
        "<div>{items.value.map(x => <span>{x}</span>)}</div>",
    );
    assert!(out.js.contains("items:"), "js:\n{}", out.js);
}

#[test]
fn value_read_only_in_if_cond_marks_state_bound() {
    let out = compile(
        "import { state } from 'mist'\nconst on = state(false)",
        "<div>{on.value ? <span>A</span> : <span>B</span>}</div>",
    );
    assert!(out.js.contains("on:"), "js:\n{}", out.js);
}

#[test]
fn real_checked_bind_still_marks_state_bound() {
    let out = compile(
        "import { state } from 'mist'\nconst on = state(false)",
        "<switch checked:bind={on} />",
    );
    assert!(out.js.contains("on: false"), "js:\n{}", out.js);
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
    assert!(out.js.contains("rt.derive(this, __o, '_h0', null, () => (this.fmt(this.data.n)), ['n'])"), "js:\n{}", out.js);
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
        out.js.contains("rt.derive(this, __o, '_hl0', 'id', () => (this.data.txs).map(t => ({ ...t, _c0: this.fmtDate(t.ts) })), ['txs'])"),
        "js:\n{}",
        out.js
    );
}

#[test]
fn m1008_warns_on_keyless_reactive_map() {
    let src = "---\nimport { state } from 'mist'\nconst items = state([])\n---\n<div>{items.value.map(t => (<span>{t.a}</span>))}</div>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert_eq!(unit.warnings.len(), 1, "warnings: {:?}", unit.warnings);
    assert!(unit.warnings[0].contains("M1008"), "warnings: {:?}", unit.warnings);
    assert!(unit.warnings[0].contains("items.value"), "warnings: {:?}", unit.warnings);
}

#[test]
fn m1008_silent_with_key() {
    let src = "---\nimport { state } from 'mist'\nconst items = state([])\n---\n<div>{items.value.map(t => (<span key={t.id}>{t.a}</span>))}</div>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert!(unit.warnings.is_empty(), "warnings: {:?}", unit.warnings);
}

#[test]
fn ts_annotations_are_stripped_from_emitted_js() {
    let src = "---\nimport { state } from 'mist'\ninterface Todo { id: number, done: boolean }\ntype Filter = 'all' | 'open'\nconst todos = state<Todo[]>([])\nconst mode = state('all' as Filter)\nfunction add(text: string, done?: boolean): void {\n  todos.value.push({ id: 1, done: !!done })\n}\n---\n<span onTap={() => add('x')}>{todos.value.length}</span>\n";
    let out = mistc::compile(src).expect("compile failed");
    assert!(!out.js.contains("interface"), "js:\n{}", out.js);
    assert!(!out.js.contains("Filter"), "js:\n{}", out.js);
    assert!(!out.js.contains(": string"), "js:\n{}", out.js);
    assert!(!out.js.contains(": void"), "js:\n{}", out.js);
    assert!(!out.js.contains("?"), "js:\n{}", out.js);
    assert!(!out.js.contains("as "), "js:\n{}", out.js);
}

#[test]
fn type_only_imports_are_dropped() {
    let src = "---\nimport { state } from 'mist'\nimport type { Todo } from './types'\nconst todos = state([])\n---\n<span>{todos.value.length}</span>\n";
    let out = mistc::compile(src).expect("compile failed");
    assert!(!out.js.contains("types"), "js:\n{}", out.js);
}

#[test]
fn m1008_warns_on_keyless_derived_list() {
    let src = "---\nimport { state, derived } from 'mist'\nconst items = state([])\nconst open = derived(() => items.value.filter(t => !t.done))\n---\n<div>{open.value.map(t => (<span>{t.a}</span>))}</div>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert_eq!(unit.warnings.len(), 1, "warnings: {:?}", unit.warnings);
    assert!(unit.warnings[0].contains("open.value"), "warnings: {:?}", unit.warnings);
}

#[test]
fn m1018_warns_on_span_with_div_child() {
    let src = "---\n---\n<span class=\"p-4 flex\"><div>x</div></span>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert_eq!(unit.warnings.len(), 1, "warnings: {:?}", unit.warnings);
    assert!(unit.warnings[0].contains("M1018"), "warnings: {:?}", unit.warnings);
    assert!(unit.warnings[0].contains("<div>"), "warnings: {:?}", unit.warnings);
}

#[test]
fn m1018_silent_on_span_with_text_expr_and_nested_span() {
    let src = "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>hi {n.value} <span>nested</span></span>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert!(unit.warnings.is_empty(), "warnings: {:?}", unit.warnings);
}

#[test]
fn m1018_warns_on_conditional_div_inside_span() {
    let src = "---\nimport { state } from 'mist'\nconst cond = state(true)\n---\n<span>{cond.value && <div>x</div>}</span>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert_eq!(unit.warnings.len(), 1, "warnings: {:?}", unit.warnings);
    assert!(unit.warnings[0].contains("M1018"), "warnings: {:?}", unit.warnings);
}

#[test]
fn m1018_warns_on_ternary_div_and_span_branches() {
    let src = "---\nimport { state } from 'mist'\nconst cond = state(true)\n---\n<span>{cond.value ? <div>a</div> : <span>b</span>}</span>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert_eq!(unit.warnings.len(), 1, "warnings: {:?}", unit.warnings);
    assert!(unit.warnings[0].contains("M1018"), "warnings: {:?}", unit.warnings);
}

#[test]
fn m1018_silent_on_anchor_with_element_children() {
    let src = "---\n---\n<a href=\"/pages/x/x\"><div>x</div></a>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert!(unit.warnings.is_empty(), "warnings: {:?}", unit.warnings);
}

#[test]
fn m1018_warns_on_literal_text_tag_with_div_child() {
    let src = "---\n---\n<text><div>x</div></text>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert_eq!(unit.warnings.len(), 1, "warnings: {:?}", unit.warnings);
    assert!(unit.warnings[0].contains("M1018"), "warnings: {:?}", unit.warnings);
}

#[test]
fn m1018_message_shows_mapping_for_translated_child_and_omits_for_passthrough() {
    let src = "---\n---\n<span class=\"p-4\"><div>x</div></span>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert!(unit.warnings[0].contains("<div> (→ <view>)"), "warnings: {:?}", unit.warnings);

    let src2 = "---\n---\n<span class=\"p-4\"><scroll-view>x</scroll-view></span>\n";
    let unit2 = mistc::compile_unit(src2, true).expect("compile failed");
    assert!(unit2.warnings[0].contains("<scroll-view>"), "warnings: {:?}", unit2.warnings);
    assert!(!unit2.warnings[0].contains("→"), "warnings: {:?}", unit2.warnings);
}

#[test]
fn m1018_warning_does_not_fail_compile() {
    let src = "---\n---\n<span class=\"p-4 flex\"><div>x</div></span>\n";
    let unit = mistc::compile_unit(src, true).expect("compile should succeed despite warning");
    assert!(unit.output.wxml.contains("<text"), "wxml:\n{}", unit.output.wxml);
    assert!(unit.output.wxml.contains("<view"), "wxml:\n{}", unit.output.wxml);
    assert!(!unit.warnings.is_empty());
}

#[test]
fn m1019_warns_on_unknown_tag_with_suggestion() {
    let src = "---\n---\n<scroll-veiw>x</scroll-veiw>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert_eq!(unit.warnings.len(), 1, "warnings: {:?}", unit.warnings);
    assert!(unit.warnings[0].contains("M1019"), "warnings: {:?}", unit.warnings);
    assert!(unit.warnings[0].contains("<scroll-veiw>"), "warnings: {:?}", unit.warnings);
    assert!(unit.warnings[0].contains("did you mean <scroll-view>?"), "warnings: {:?}", unit.warnings);
}

#[test]
fn m1019_silent_on_native_tags() {
    let src = "---\n---\n<scroll-view><swiper><swiper-item>x</swiper-item></swiper></scroll-view>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert!(unit.warnings.is_empty(), "warnings: {:?}", unit.warnings);
}

#[test]
fn m1019_silent_on_web_alias_tags() {
    let src = "---\n---\n<div><span>x</span></div>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert!(unit.warnings.is_empty(), "warnings: {:?}", unit.warnings);
}

#[test]
fn m1019_silent_on_registered_component() {
    let src = "---\nimport Badge from '../components/badge.mist'\n---\n<Badge />\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert!(
        unit.warnings.iter().all(|w| !w.contains("M1019")),
        "warnings: {:?}",
        unit.warnings
    );
}

#[test]
fn m1019_silent_on_manual_using_components_tag() {
    let src = "---\nimport { state } from 'mist'\nconst n = state(0)\nexport const config = { usingComponents: { 'van-button': '@vant/weapp/button/index' } }\n---\n<van-button>{n.value}</van-button>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert!(
        unit.warnings.iter().all(|w| !w.contains("M1019")),
        "warnings: {:?}",
        unit.warnings
    );
}

#[test]
fn m1019_silent_when_suppressed_by_custom_tags() {
    let src = "---\nexport const config = { customTags: ['scroll-veiw'] }\n---\n<scroll-veiw>x</scroll-veiw>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert!(
        unit.warnings.iter().all(|w| !w.contains("M1019")),
        "warnings: {:?}",
        unit.warnings
    );
}

#[test]
fn m1019_dedupes_repeats_of_the_same_unknown_tag() {
    let src = "---\n---\n<div><swipper /><swipper /></div>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    let m1019_count = unit.warnings.iter().filter(|w| w.contains("M1019")).count();
    assert_eq!(m1019_count, 1, "warnings: {:?}", unit.warnings);
}

#[test]
fn m1019_warning_does_not_fail_compile() {
    let src = "---\n---\n<scroll-veiw>x</scroll-veiw>\n";
    let unit = mistc::compile_unit(src, true).expect("compile should succeed despite warning");
    assert!(unit.output.wxml.contains("<scroll-veiw>"), "wxml:\n{}", unit.output.wxml);
    assert!(!unit.warnings.is_empty());
}

#[test]
fn m1008_propagates_through_project_build() {
    let dir = std::env::temp_dir().join("mist-m1008-prop");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let page = dir.join("keyless.mist");
    std::fs::write(&page, "---\nimport { state } from 'mist'\nconst items = state([])\n---\n<div>{items.value.map(t => (<span>{t.a}</span>))}</div>\n").expect("write page");
    let project = mistc::compile_project(&page).expect("project build failed");
    assert_eq!(project.warnings.len(), 1, "warnings: {:?}", project.warnings);
    assert!(project.warnings[0].contains("M1008"), "warnings: {:?}", project.warnings);
    assert!(project.warnings[0].contains("keyless.mist"), "warnings: {:?}", project.warnings);
}

#[test]
fn ts_satisfies_nonnull_and_arrow_returns_stripped() {
    let src = "---\nimport { state } from 'mist'\nconst items = state([])\nconst limit = { max: 5 } satisfies { max: number }\nfunction first(): void {\n  const f = (n: number): number => n + 1\n  items.value[0]!.done = f(1) > 0\n}\n---\n<span>{items.value.length}</span>\n";
    let out = mistc::compile(src).expect("compile failed");
    assert!(!out.js.contains("satisfies"), "js:\n{}", out.js);
    assert!(!out.js.contains(": number"), "js:\n{}", out.js);
    assert!(!out.js.contains("!."), "js:\n{}", out.js);
}

#[test]
fn ts_annotations_stripped_in_lifecycles() {
    let src = "---\nimport { state } from 'mist'\nconst n = state(0)\nonLoad((q: { id: string }) => {\n  n.value = 1\n})\n---\n<span>{n.value}</span>\n";
    let out = mistc::compile(src).expect("compile failed");
    assert!(!out.js.contains(": string"), "js:\n{}", out.js);
}

#[test]
fn ts_annotations_stripped_in_store_modules() {
    let (js, _) = mistc::frontmatter::compile_store_module(
        "import { store } from 'mist'\ninterface User { name: string }\nexport const user = store<User>({ name: '' })\nexport function rename(n: string): void { user.value.name = n }\n",
        "./mist-rt.js",
    )
    .expect("store compile failed");
    assert!(!js.contains("interface"), "js:\n{}", js);
    assert!(!js.contains(": string"), "js:\n{}", js);
    assert!(!js.contains(": void"), "js:\n{}", js);
}

#[test]
fn derive_emits_dependency_lists() {
    let src = "---\nimport { state, derived } from 'mist'\nconst todos = state([])\nconst filter = state('all')\nconst open = derived(() => filter.value === 'all' ? todos.value : todos.value.filter(t => !t.done))\n---\n<div>{open.value.map(t => (<span key={t.id}>{t.a}</span>))}</div>\n";
    let out = mistc::compile(src).expect("compile failed");
    assert!(out.js.contains(", ['filter', 'todos']);"), "js:\n{}", out.js);
}

#[test]
fn derive_deps_resolve_through_pure_local_methods() {
    let src = "---\nimport { state, derived } from 'mist'\nconst n = state(0)\nfunction bump() { return 1 }\nconst d = derived(() => n.value + bump())\n---\n<span>{d.value}</span>\n";
    let out = mistc::compile(src).expect("compile failed");
    assert!(out.js.contains("this.bump(), ['n']);"), "js:\n{}", out.js);
}

#[test]
fn class_list_compiles_static_conditional_and_object_entries() {
    let src = "---\nimport { state } from 'mist'\nconst open = state(false)\nconst done = state(false)\n---\n<div class:list={['p-4 font-bold', open.value && 'text-red-500', { hidden: done.value, 'w-[32px]': open.value }]}>x</div>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    let wxml = &unit.output.wxml;
    assert!(wxml.contains("class=\"p-4 font-bold {{(open && 'text-red-500') || ''}} {{done ? 'hidden' : ''}} {{open ? 'w-_32px_' : ''}}\""), "wxml:\n{}", wxml);
    for c in ["p-4", "font-bold", "text-red-500", "hidden", "w-[32px]"] {
        assert!(unit.classes.iter().any(|x| x == c), "missing class {} in {:?}", c, unit.classes);
    }
}

#[test]
fn class_and_class_list_together_error() {
    let src = "---\nimport { state } from 'mist'\nconst open = state(false)\n---\n<div class=\"p-4\" class:list={[open.value && 'hidden']}>x</div>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("class:list"), "err: {}", err);
}

#[test]
fn class_list_requires_array_literal() {
    let src = "---\nimport { state } from 'mist'\nconst open = state(false)\n---\n<div class:list={open.value}>x</div>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("array literal"), "err: {}", err);
}

#[test]
fn config_inline_key_never_reaches_emitted_json() {
    let src = "---\nimport { state } from 'mist'\nconst n = state(0)\nexport const config = { navigationBarTitleText: 'X', inline: false }\n---\n<span>{n.value}</span>\n";
    let out = mistc::compile(src).expect("compile failed");
    let json = out.json.expect("json missing");
    assert!(!json.contains("inline"), "json:\n{}", json);
    assert!(json.contains("navigationBarTitleText"), "json:\n{}", json);
}

#[test]
fn config_only_inline_emits_no_config_keys() {
    let src = "---\nimport { props } from 'mist'\nconst { label } = props()\nexport const config = { inline: false }\n---\n<span>{label}</span>\n";
    let unit = mistc::compile_unit(src, false).expect("compile failed");
    let json = unit.output.json.expect("json missing");
    assert!(!json.contains("inline"), "json:\n{}", json);
}

#[test]
fn hoisted_deriveds_get_deps_through_pure_methods() {
    let src = "---\nimport { state } from 'mist'\nconst n = state(1500)\nfunction fmt(x) { return '¥' + x }\n---\n<span>{fmt(n.value)}</span>\n";
    let out = mistc::compile(src).expect("compile failed");
    assert!(out.js.contains("rt.derive(this, __o, '_h0', null, () => (this.fmt(this.data.n)), ['n']);"), "js:\n{}", out.js);
}

#[test]
fn impure_method_calls_keep_null_deps() {
    let src = "---\nimport { state } from 'mist'\nconst n = state(0)\nconst m = state(1)\nfunction shady(x) { return x + this_free() + this.data.m }\nfunction this_free() { return 1 }\n---\n<span>{shady(n.value)}</span><span>{m.value}</span>\n";
    let out = mistc::compile(src).expect("compile failed");
    assert!(out.js.contains("(this.shady(this.data.n)), null);"), "js:\n{}", out.js);
}

#[test]
fn wxml_hostile_expressions_are_hoisted() {
    let src = "---\nimport { state } from 'mist'\nconst n = state(1)\nconst o = state(null)\nfunction fmt(x) { return x }\n---\n<div>\n  <span>{fmt(5)}</span>\n  <span>{`x ${n.value}`}</span>\n  <span>{o.value?.a?.b}</span>\n  <span>{Math.max(1, n.value)}</span>\n</div>\n";
    let out = mistc::compile(src).expect("compile failed");
    for h in ["_h0", "_h1", "_h2", "_h3"] {
        assert!(out.wxml.contains(&format!("{{{{{}}}}}", h)), "wxml missing {}:\n{}", h, out.wxml);
    }
    assert!(!out.wxml.contains("fmt("), "raw call leaked:\n{}", out.wxml);
    assert!(!out.wxml.contains('`'), "template literal leaked:\n{}", out.wxml);
    assert!(!out.wxml.contains("?."), "optional chaining leaked:\n{}", out.wxml);
    assert!(out.js.contains("Math.max(1, this.data.n)"), "js:\n{}", out.js);
}

#[test]
fn hostile_loop_item_expression_errors() {
    let src = "---\nimport { state } from 'mist'\nconst items = state([])\n---\n<div>{items.value.map(t => (<span key={t.id}>{`v-${t.name}`}</span>))}</div>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1009"), "err: {}", err);
    assert!(err.contains("help:"), "err: {}", err);
}

#[test]
fn double_quotes_in_attr_expressions_are_escaped() {
    let src = "---\nimport { state } from 'mist'\nconst on = state(true)\n---\n<div title={on.value ? \"a\" : \"b\"}>x</div>\n";
    let out = mistc::compile(src).expect("compile failed");
    assert!(out.wxml.contains("title=\"{{on ? &quot;a&quot; : &quot;b&quot;}}\""), "wxml:\n{}", out.wxml);
}

#[test]
fn deriveds_emit_in_dependency_order() {
    let src = "---\nimport { state, derived } from 'mist'\nconst x = state(1)\nconst b = derived(() => a.value + 1)\nconst a = derived(() => x.value * 2)\n---\n<span>{b.value}{a.value}</span>\n";
    let out = mistc::compile(src).expect("compile failed");
    let a_pos = out.js.find("__o, 'a'").expect("a missing");
    let b_pos = out.js.find("__o, 'b'").expect("b missing");
    assert!(a_pos < b_pos, "a must be computed before b:\n{}", out.js);
    assert!(out.js.contains("__o, 'b', null, () => this.data.a + 1, ['a']"), "js:\n{}", out.js);
}

#[test]
fn cyclic_deriveds_keep_source_order() {
    let src = "---\nimport { state, derived } from 'mist'\nconst x = state(1)\nconst a = derived(() => (b.value || 0) + x.value)\nconst b = derived(() => (a.value || 0) + 1)\n---\n<span>{a.value}{b.value}</span>\n";
    let out = mistc::compile(src).expect("compile failed");
    let a_pos = out.js.find("__o, 'a'").expect("a missing");
    let b_pos = out.js.find("__o, 'b'").expect("b missing");
    assert!(a_pos < b_pos, "cycle keeps source order:\n{}", out.js);
}

#[test]
fn interaction_lifecycle_hooks_emit_into_page() {
    let src = "---\nimport { state, onPullDownRefresh, onReachBottom, onTabItemTap } from 'mist'\nconst n = state(0)\nonPullDownRefresh(() => {\n  n.value = 0\n  wx.stopPullDownRefresh()\n})\nonReachBottom(() => { n.value++ })\nonTabItemTap((item) => { n.value = item.index })\n---\n<span>{n.value}</span>\n";
    let out = mistc::compile(src).expect("compile failed");
    assert!(out.js.contains("onPullDownRefresh() {"), "js:\n{}", out.js);
    assert!(out.js.contains("onReachBottom() {"), "js:\n{}", out.js);
    assert!(out.js.contains("onTabItemTap(item) {"), "js:\n{}", out.js);
    assert!(out.js.contains("this.__set('n', 0)"), "mutations must compile inside hooks:\n{}", out.js);
}

#[test]
fn share_hooks_return_their_config() {
    let src = "---\nimport { state, onShareAppMessage, onShareTimeline } from 'mist'\nconst title = state('hi')\nonShareAppMessage(() => ({ title: title.value, path: '/pages/index/index' }))\nonShareTimeline(() => {\n  return { title: title.value }\n})\n---\n<span>{title.value}</span>\n";
    let out = mistc::compile(src).expect("compile failed");
    assert!(out.js.contains("onShareAppMessage() { return ({ title: this.data.title, path: '/pages/index/index' }); }"), "js:\n{}", out.js);
    assert!(out.js.contains("onShareTimeline() {"), "js:\n{}", out.js);
    assert!(out.js.contains("return { title: this.data.title }"), "js:\n{}", out.js);
}

#[test]
fn route_done_and_save_exit_state_emit_in_page() {
    let src = "---\nimport { state, onRouteDone, onSaveExitState } from 'mist'\nconst n = state(0)\nonRouteDone(() => { n.value++ })\nonSaveExitState(() => {\n  return { data: { n: n.value }, expireTimeStamp: 0 }\n})\n---\n<span>{n.value}</span>\n";
    let out = mistc::compile(src).expect("compile failed");
    assert!(out.js.contains("onRouteDone() {"), "js:\n{}", out.js);
    assert!(out.js.contains("onSaveExitState() {"), "js:\n{}", out.js);
    assert!(out.js.contains("return { data: { n: this.data.n }, expireTimeStamp: 0 }"), "js:\n{}", out.js);
}

#[test]
fn page_lifetime_hooks_map_into_component_pagelifetimes() {
    let src = "---\nimport { props, state, onPageShow, onPageHide } from 'mist'\nconst { label } = props()\nconst n = state(0)\nfunction bump() { n.value++ }\nonPageShow(() => { n.value++ })\nonPageHide(() => { n.value = 0 })\n---\n<span onTap={bump}>{label}{n.value}</span>\n";
    let unit = mistc::compile_unit(src, false).expect("compile failed");
    let js = &unit.output.js;
    assert!(js.contains("pageLifetimes: {"), "js:\n{}", js);
    assert!(js.contains("show() {"), "js:\n{}", js);
    assert!(js.contains("hide() {"), "js:\n{}", js);
    assert!(!js.contains("onPageShow"), "mist name must not leak:\n{}", js);
}

#[test]
fn page_lifetime_hooks_rejected_in_pages() {
    let src = "---\nimport { state, onPageShow } from 'mist'\nconst n = state(0)\nonPageShow(() => { n.value++ })\n---\n<span>{n.value}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("component-only"), "err: {}", err);
    assert!(err.contains("onShow"), "err: {}", err);
}

#[test]
fn page_only_hooks_rejected_in_components() {
    let src = "---\nimport { props, state, onPullDownRefresh } from 'mist'\nconst { label } = props()\nconst n = state(0)\nonPullDownRefresh(() => { n.value = 0 })\n---\n<span>{label}{n.value}</span>\n";
    let err = match mistc::compile_unit(src, false) {
        Err(e) => e,
        Ok(_) => panic!("expected page-only hook error"),
    };
    assert!(err.contains("page-only"), "err: {}", err);
}

#[test]
fn on_resize_allowed_in_pages_and_maps_in_components() {
    let page = "---\nimport { state, onResize } from 'mist'\nconst n = state(0)\nonResize((e) => { n.value = e.size.windowWidth })\n---\n<span>{n.value}</span>\n";
    let out = mistc::compile(page).expect("page compile failed");
    assert!(out.js.contains("onResize(e) {"), "js:\n{}", out.js);
    let comp = "---\nimport { props, state, onResize } from 'mist'\nconst { label } = props()\nconst n = state(0)\nonResize(() => { n.value++ })\n---\n<span>{label}{n.value}</span>\n";
    let unit = mistc::compile_unit(comp, false).expect("component compile failed");
    assert!(unit.output.js.contains("pageLifetimes: {"), "js:\n{}", unit.output.js);
    assert!(unit.output.js.contains("resize() {"), "js:\n{}", unit.output.js);
}

#[test]
fn member_calls_matching_method_names_are_not_rewritten() {
    let src = "---\nimport { state } from 'mist'\nconst draft = state('')\nfunction send() {\n  const socket = wx.connectSocket({ url: 'wss://x' })\n  socket.send({ data: draft.value })\n  send2()\n}\nfunction send2() { wx.showToast({ title: draft.value }) }\n---\n<span onTap={send}>{draft.value}</span>\n";
    let out = mistc::compile(src).expect("compile failed");
    assert!(out.js.contains("socket.send({ data: this.data.draft })"), "js:\n{}", out.js);
    assert!(out.js.contains("this.send2()"), "bare method calls must still rewrite:\n{}", out.js);
    assert!(!out.js.contains(".this."), "corrupted member call:\n{}", out.js);
}

#[test]
fn method_call_inside_template_literal_interpolation_is_rewritten() {
    let src = "---\nimport { state } from 'mist'\nconst n = state(2)\nfunction fmt(v) { return v.toFixed(2) }\nfunction show() {\n  wx.showToast({ title: `total: ${fmt(n.value)}` })\n}\n---\n<span onTap={show}>{n.value}</span>\n";
    let out = mistc::compile(src).expect("compile failed");
    assert!(out.js.contains("${this.fmt(this.data.n)}"), "js:\n{}", out.js);
    assert!(!out.js.contains(".this."), "corrupted member call:\n{}", out.js);
}

#[test]
fn plugin_import_emits_require_plugin() {
    let src = "---\nimport cal from 'plugin://calendar'\nimport { state } from 'mist'\nconst n = state(0)\nfunction open() {\n  cal.select()\n}\n---\n<span onTap={open}>{n.value}</span>\n";
    let out = mistc::compile(src).expect("compile failed");
    assert!(out.js.contains("const cal = requirePlugin('calendar');"), "js:\n{}", out.js);
    assert!(out.js.contains("cal.select()"), "js:\n{}", out.js);
}

#[test]
fn unbound_state_init_calling_frontmatter_function_is_rewritten() {
    let out = compile(
        "import { state, derived } from 'mist'\nfunction seed() { return [1, 2, 3] }\nconst items = state(seed())\nconst count = derived(() => items.value.length)",
        "<span>{count.value}</span>",
    );
    assert!(out.js.contains("this._items = this.seed()"), "js:\n{}", out.js);
    assert!(!out.js.contains("= seed()"), "bare init call must not survive:\n{}", out.js);
}

#[test]
fn scoped_style_suffixes_selectors_and_markup() {
    let out = compile(
        "import { state } from 'mist'\nconst on = state(false)\nfunction tap() { on.value = true }",
        "<div class=\"card p-4\"><span class={on.value ? 'card big' : 'dim'}>x</span><div hover-class=\"card\" onTap={tap}>t</div></div>\n<style scoped>\n.card { color: red; }\n.big { font-weight: bold; }\n@media (min-width: 600px) { .card { color: blue; } }\n@keyframes spin { from { opacity: 0; } }\n</style>",
    );
    assert!(out.wxss.contains(".card--unit {"), "wxss:\n{}", out.wxss);
    assert!(out.wxss.contains(".card--unit { color: blue"), "media selectors must scope:\n{}", out.wxss);
    assert!(out.wxss.contains("@keyframes spin { from"), "keyframes must stay untouched:\n{}", out.wxss);
    assert!(out.wxml.contains("class=\"card--unit p-4\""), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("'card--unit big--unit'"), "ternary literals must scope:\n{}", out.wxml);
    assert!(out.wxml.contains("'dim'"), "unscoped names stay:\n{}", out.wxml);
    assert!(out.wxml.contains("hover-class=\"card--unit\""), "hover-class must scope:\n{}", out.wxml);
}

#[test]
fn scoped_style_rewrites_hoisted_class_expressions() {
    let out = compile(
        "import { state } from 'mist'\nconst on = state(false)\nfunction extra() { return on.value ? 'big' : '' }",
        "<div class={on.value ? `card ${extra()}` : 'dim'}>x</div>\n<style scoped>\n.card { color: red; }\n.dim { opacity: 0.5; }\n</style>",
    );
    assert!(out.wxml.contains("class=\"{{_h0}}\""), "class expr must hoist:\n{}", out.wxml);
    assert!(out.js.contains("card--unit"), "hoisted class literal must scope:\n{}", out.js);
    assert!(out.js.contains("'dim--unit'"), "hoisted ternary literal must scope:\n{}", out.js);
}

#[test]
fn scoped_style_survives_comments_with_stray_braces() {
    let out = compile(
        "import { state } from 'mist'\nconst n = state(0)",
        "<div class=\"card\">{n.value}</div>\n<style scoped>\n/* TODO: wrap in { later */\n.card { color: red; }\n/* trailing } note */\n.big { font-weight: bold; }\n</style>",
    );
    assert!(out.wxss.contains(".card--unit { color: red"), "wxss:\n{}", out.wxss);
    assert!(out.wxss.contains(".big--unit { font-weight: bold"), "wxss:\n{}", out.wxss);
    assert!(!out.wxss.contains("TODO"), "comments must be stripped:\n{}", out.wxss);
}

#[test]
fn scoped_style_ignores_strings_in_css() {
    let out = compile(
        "import { state } from 'mist'\nconst n = state(0)",
        "<div class=\"card\">{n.value}</div>\n<style scoped>\n.card { content: \"}\"; color: red; }\n[data-x=\".card\"] { color: green; }\n.big { font-weight: bold; }\n</style>",
    );
    assert!(out.wxss.contains(".card--unit { content: \"}\"; color: red"), "brace in string must not end the rule:\n{}", out.wxss);
    assert!(out.wxss.contains("[data-x=\".card\"] { color: green"), "class-like text in attr-selector strings must stay:\n{}", out.wxss);
    assert!(out.wxss.contains(".big--unit { font-weight: bold"), "wxss:\n{}", out.wxss);
}

#[test]
fn scoped_style_anchors_class_attributes() {
    let out = compile(
        "import { state } from 'mist'\nconst n = state(0)",
        "<div subclass=\"card\"><input placeholder-class=\"ph\" />{n.value}</div>\n<style scoped>\n.card { color: red; }\n.ph { color: gray; }\n</style>",
    );
    assert!(out.wxml.contains("subclass=\"card\""), "non-class attrs must stay untouched:\n{}", out.wxml);
    assert!(out.wxml.contains("placeholder-class=\"ph--unit\""), "placeholder-class must scope:\n{}", out.wxml);
}

#[test]
fn unscoped_style_stays_verbatim() {
    let out = compile(
        "import { state } from 'mist'\nconst n = state(0)",
        "<div class=\"card\">{n.value}</div>\n<style>\n.card { color: red; }\n</style>",
    );
    assert!(out.wxss.contains(".card {"), "wxss:\n{}", out.wxss);
    assert!(out.wxml.contains("class=\"card\""), "wxml:\n{}", out.wxml);
}
