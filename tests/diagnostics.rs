#[test]
fn m1004_reports_file_line_of_the_mutation() {
    // line 1: ---, line 2: import, line 3: const, line 4: function, line 5: splice
    let src = "---\nimport { state } from 'mist'\nconst items = state([])\nfunction rm(i) {\n  items.value.splice(i, 1)\n}\n---\n<span>{items.value.length}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1004 at line 5:3"), "err: {}", err);
    assert!(err.contains("help:"), "err: {}", err);
}

#[test]
fn m1003_computed_key_errors() {
    let src = "---\nimport { state } from 'mist'\nconst items = state([])\n---\n<div>{items.value.map(t => (<span key={t.a + t.b}>{t.a}</span>))}</div>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1003"), "err: {}", err);
    assert!(err.contains("direct property"), "err: {}", err);
}

#[test]
fn m1003_deep_key_path_errors() {
    let src = "---\nimport { state } from 'mist'\nconst items = state([])\n---\n<div>{items.value.map(t => (<span key={t.meta.id}>{t.a}</span>))}</div>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1003"), "err: {}", err);
}

#[test]
fn m1003_multiple_keys_in_loop_body_errors() {
    let src = "---\nimport { state } from 'mist'\nconst items = state([])\n---\n<div>{items.value.map(t => (<div><span key={t.id}>{t.a}</span><span key={t.b}>{t.a}</span></div>))}</div>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1003"), "err: {}", err);
    assert!(err.contains("multiple key"), "err: {}", err);
}

#[test]
fn template_errors_report_file_line() {
    // template starts on line 5; the bad closing tag is on line 7
    let src = "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<div>\n  <span>{n.value}\n</div>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1010 at line 7"), "err: {}", err);
}

#[test]
fn jsx_ternary_with_one_non_jsx_branch_errors() {
    // line 1: ---, line 2: import, line 3: const, line 4: ---, line 5: template
    let src = "---\nimport { state } from 'mist'\nconst on = state(false)\n---\n<div>{on.value ? <span>A</span> : 'b'}</div>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1010 at line 5"), "err: {}", err);
    assert!(err.contains("both branches of a JSX ternary"), "err: {}", err);
}

#[test]
fn jsx_ternary_with_colon_attr_then_branch_and_non_jsx_else_still_errors() {
    // line 1: ---, line 2: import, line 3: const a, line 4: const x, line 5: ---, line 6: template
    let src = "---\nimport { state } from 'mist'\nconst a = state(false)\nconst x = state('')\n---\n<div>{a.value ? <input value:bind={x} /> : 'text'}</div>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1010 at line 6"), "err: {}", err);
    assert!(err.contains("both branches of a JSX ternary"), "err: {}", err);
}

#[test]
fn m1005_collision_is_coded() {
    let src = "---\nimport { state } from 'mist'\nconst n = state(0)\nfunction n() {}\n---\n<span>{n.value}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1005"), "err: {}", err);
}

#[test]
fn line_col_helper_is_accurate() {
    let src = "abc\ndefg\nhi";
    assert_eq!(mistc::frontmatter::line_col(src, 0, 1), (1, 1));
    assert_eq!(mistc::frontmatter::line_col(src, 4, 1), (2, 1));
    assert_eq!(mistc::frontmatter::line_col(src, 6, 1), (2, 3));
    assert_eq!(mistc::frontmatter::line_col(src, 9, 10), (12, 1));
}

#[test]
fn m1001_alias_member_write_errors() {
    let src = "---\nimport { state } from 'mist'\nconst todos = state([])\nfunction toggle(i) {\n  const t = todos.value[0]\n  t.done = true\n}\n---\n<span>{todos.value.length}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1001 at line 6:3"), "err: {}", err);
    assert!(err.contains("alias of `todos.value[0]`"), "err: {}", err);
    assert!(err.contains("help:"), "err: {}", err);
}

#[test]
fn m1001_alias_mutating_call_errors() {
    let src = "---\nimport { state } from 'mist'\nconst todos = state([])\nfunction add(x) {\n  const t = todos.value\n  t.push(x)\n}\n---\n<span>{todos.value.length}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1001 at line 6:3"), "err: {}", err);
    assert!(err.contains("t.push()"), "err: {}", err);
}

#[test]
fn m1001_alias_update_expression_errors() {
    let src = "---\nimport { state } from 'mist'\nconst user = state({ visits: 0 })\nfunction bump() {\n  const u = user.value\n  u.visits++\n}\n---\n<span>{user.value.visits}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1001 at line 6:3"), "err: {}", err);
}

#[test]
fn m1001_read_only_alias_is_fine() {
    let src = "---\nimport { state } from 'mist'\nconst todos = state([])\nconst count = state(0)\nfunction f() {\n  const t = todos.value\n  count.value = t.length\n}\n---\n<span>{count.value}</span>\n";
    assert!(mistc::compile(src).is_ok());
}

#[test]
fn m1001_skips_reassigned_locals() {
    let src = "---\nimport { state } from 'mist'\nconst todos = state([])\nfunction f(other) {\n  let t = todos.value\n  t = other\n  t.done = true\n}\n---\n<span>{todos.value.length}</span>\n";
    assert!(mistc::compile(src).is_ok());
}

#[test]
fn m1001_skips_shadowed_names() {
    let src = "---\nimport { state } from 'mist'\nconst todos = state([])\nconst t = todos.value\nfunction f(t) {\n  t.done = true\n}\n---\n<span>{todos.value.length}</span>\n";
    assert!(mistc::compile(src).is_ok());
}

#[test]
fn m1001_copies_are_not_aliases() {
    let src = "---\nimport { state } from 'mist'\nconst todos = state([])\nfunction f() {\n  const t = todos.value.slice()\n  t.push(1)\n}\n---\n<span>{todos.value.length}</span>\n";
    assert!(mistc::compile(src).is_ok());
}

#[test]
fn m1001_fires_in_store_modules() {
    let err = mistc::frontmatter::compile_store_module(
        "import { store } from 'mist'\nexport const user = store({ name: '' })\nexport function rename(n) {\n  const u = user.value\n  u.name = n\n}\n",
        "./mist-rt.js",
    )
    .unwrap_err();
    assert!(err.contains("M1001 at line 5:3"), "err: {}", err);
    assert!(err.contains("alias of `user.value`"), "err: {}", err);
}

#[test]
fn m1001_fires_for_unbound_state_aliases() {
    let src = "---\nimport { state } from 'mist'\nconst hidden = state({ n: 0 })\nconst shown = state(0)\nfunction f() {\n  const h = hidden.value\n  h.n = 1\n}\n---\n<span>{shown.value}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1001 at line 7:3"), "err: {}", err);
}

#[test]
fn npm_default_import_emits_vendor_require() {
    let src = "---\nimport { state } from 'mist'\nimport dayjs from 'dayjs'\nconst n = state(0)\nfunction stamp() {\n  n.value = dayjs('2026-01-01').valueOf()\n}\n---\n<span onTap={stamp}>{n.value}</span>\n";
    let out = mistc::compile(src).expect("npm imports must compile");
    assert!(out.js.contains("const dayjs = __npmi(require('./dayjs.js'));"), "js:\n{}", out.js);
}

#[test]
fn m1015_named_plugin_import_errors() {
    let src = "---\nimport { x } from 'plugin://calendar'\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1015"), "err: {}", err);
}

#[test]
fn m1015_empty_plugin_name_errors() {
    let src = "---\nimport p from 'plugin://'\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1015"), "err: {}", err);
}

#[test]
fn m1026_catches_spread_arguments() {
    let src = "---\nimport { state } from 'mist'\nimport { pick } from 'toolkit'\nconst items = state([1, 2])\nfunction f() {\n  return pick(...items.value)\n}\n---\n<span>{f()}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1026") && err.contains("'items'"), "err: {}", err);
}

#[test]
fn m1026_reactive_value_into_npm_import_errors() {
    let src = "---\nimport { state } from 'mist'\nimport dayjs from 'dayjs'\nconst when = state({ ts: 0 })\nfunction fmt() {\n  return dayjs(when.value)\n}\n---\n<span>{fmt()}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(
        err.contains("M1026") && err.contains("'when'") && err.contains("'dayjs'"),
        "err: {}",
        err
    );
}

#[test]
fn m1009_call_in_nested_loop_errors() {
    let src = "---\nimport { state } from 'mist'\nconst groups = state([])\nfunction fmt(s) { return s }\n---\n<div>{groups.value.map(g => (<div key={g.id}>{g.items.map(it => (<span key={it.id}>{fmt(it.name)}</span>))}</div>))}</div>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1009"), "err: {}", err);
    assert!(err.contains("help:"), "err: {}", err);
}

#[test]
fn nested_loop_without_calls_still_compiles() {
    let src = "---\nimport { state } from 'mist'\nconst groups = state([])\n---\n<div>{groups.value.map(g => (<div key={g.id}>{g.items.map(it => (<span key={it.id}>{it.name}</span>))}</div>))}</div>\n";
    let out = mistc::compile(src).expect("compile failed");
    assert!(out.wxml.contains("wx:for=\"{{g.items}}\""), "wxml:\n{}", out.wxml);
}

#[test]
fn m1007_bare_state_use_errors() {
    let src = "---\nimport { state } from 'mist'\nconst count = state(0)\nfunction inc() {\n  count++\n}\n---\n<span>{count.value}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1007 at line 5:3"), "err: {}", err);
    assert!(err.contains("count.value"), "err: {}", err);
}

#[test]
fn m1007_bare_derived_read_errors() {
    let src = "---\nimport { state, derived } from 'mist'\nconst items = state([])\nconst total = derived(() => items.value.length)\nfunction f() { return total + 1 }\n---\n<span>{total.value}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1007"), "err: {}", err);
    assert!(err.contains("total"), "err: {}", err);
}

#[test]
fn m1007_skips_shadowed_params() {
    let src = "---\nimport { state } from 'mist'\nconst count = state(0)\nfunction render(count) { return count + 1 }\nfunction inc() { count.value++ }\n---\n<span>{count.value}</span>\n";
    assert!(mistc::compile(src).is_ok());
}

#[test]
fn m1007_fires_in_store_modules() {
    let err = mistc::frontmatter::compile_store_module(
        "import { store } from 'mist'\nexport const total = store(0)\nexport function bump() {\n  total++\n}\n",
        "./mist-rt.js",
    )
    .unwrap_err();
    assert!(err.contains("M1007 at line 4:3"), "err: {}", err);
}

#[test]
fn m1001_for_of_mutation_errors() {
    let src = "---\nimport { state } from 'mist'\nconst todos = state([])\nfunction done() {\n  for (const t of todos.value) {\n    t.done = true\n  }\n}\n---\n<span>{todos.value.length}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1001 at line 6:5"), "err: {}", err);
    assert!(err.contains("todos.value[…]"), "err: {}", err);
}

#[test]
fn m1001_foreach_mutation_errors() {
    let src = "---\nimport { state } from 'mist'\nconst todos = state([])\nfunction done() {\n  todos.value.forEach(t => { t.done = true })\n}\n---\n<span>{todos.value.length}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1001"), "err: {}", err);
    assert!(err.contains("todos.value[…]"), "err: {}", err);
}

#[test]
fn m1001_readonly_iteration_is_fine() {
    let src = "---\nimport { state, derived } from 'mist'\nconst todos = state([])\nconst open = derived(() => todos.value.filter(t => !t.done))\nconst ids = derived(() => todos.value.map(t => t.id))\nfunction count() {\n  let n = 0\n  for (const t of todos.value) {\n    if (t.done) { n = n + 1 }\n  }\n  return n\n}\n---\n<span>{open.value.length}{ids.value.length}</span>\n";
    assert!(mistc::compile(src).is_ok());
}

#[test]
fn m1007_shadow_in_one_fn_does_not_mask_bug_in_another() {
    let src = "---\nimport { state } from 'mist'\nconst count = state(0)\nfunction helper(count) { return count + 1 }\nfunction inc() {\n  count++\n}\n---\n<span>{count.value}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1007 at line 6:3"), "err: {}", err);
}

#[test]
fn m1007_skips_fn_local_let_shadow() {
    let src = "---\nimport { state } from 'mist'\nconst count = state(0)\nfunction f() {\n  let count = 0\n  count++\n  return count\n}\nfunction inc() { count.value++ }\n---\n<span>{count.value}</span>\n";
    assert!(mistc::compile(src).is_ok());
}

#[test]
fn ts_enum_is_rejected_with_help() {
    let src = "---\nimport { state } from 'mist'\nenum Status { Open, Done }\nconst n = state(0)\n---\n<span>{n.value}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("enum at line 3:1"), "err: {}", err);
    assert!(err.contains("help:"), "err: {}", err);
}

#[test]
fn m1007_bare_state_in_template_errors() {
    let src = "---\nimport { state } from 'mist'\nconst count = state(0)\n---\n<span>{count}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1007"), "err: {}", err);
    assert!(err.contains("count.value"), "err: {}", err);
}

#[test]
fn m1007_bare_state_in_template_event_errors() {
    let src = "---\nimport { state } from 'mist'\nconst count = state(0)\nfunction set(n) { count.value = n }\n---\n<span onTap={() => set(count + 1)}>{count.value}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1007"), "err: {}", err);
}

#[test]
fn m1007_template_allows_value_refs_params_and_strings() {
    let src = "---\nimport { state, derived } from 'mist'\nconst count = state(0)\nconst items = state([])\nconst open = derived(() => items.value.filter(t => !t.done))\n---\n<div>\n  <span>{count.value}</span>\n  <span>{'count'}</span>\n  <div>{open.value.map(count => (<span key={count.id}>{count.text}</span>))}</div>\n  {count.value > 0 && (<span>has</span>)}\n</div>\n";
    assert!(mistc::compile(src).is_ok());
}

#[test]
fn m1001_all_iterator_methods_flag_mutations() {
    for method in ["map", "filter", "find", "some", "every", "flatMap"] {
        let src = format!(
            "---\nimport {{ state }} from 'mist'\nconst todos = state([])\nfunction f() {{\n  todos.value.{}(t => {{ t.done = true }})\n}}\n---\n<span>{{todos.value.length}}</span>\n",
            method
        );
        let err = mistc::compile(&src).unwrap_err();
        assert!(err.contains("M1001"), "{}: err: {}", method, err);
    }
}

#[test]
fn m1001_push_through_for_of_alias_errors() {
    let src = "---\nimport { state } from 'mist'\nconst todos = state([])\nfunction f() {\n  for (const t of todos.value) { t.tags.push(1) }\n}\n---\n<span>{todos.value.length}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1001"), "err: {}", err);
}

#[test]
fn m1001_alias_detected_despite_same_name_param_elsewhere() {
    let src = "---\nimport { state, derived } from 'mist'\nconst todos = state([])\nconst open = derived(() => todos.value.filter(t => !t.done))\nfunction bad() { const t = todos.value[0]; t.done = true }\n---\n<span>{open.value.length}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1001"), "err: {}", err);
}

#[test]
fn npm_scoped_and_subpath_imports_compile_but_mist_subpaths_do_not() {
    for spec in ["@scope/pkg", "dayjs/plugin/utc"] {
        let src = format!(
            "---\nimport {{ state }} from 'mist'\nimport x from '{}'\nconst n = state(0)\nfunction f() {{ n.value = x(1) }}\n---\n<span onTap={{f}}>{{n.value}}</span>\n",
            spec
        );
        let out = mistc::compile(&src).unwrap_or_else(|e| panic!("{}: {}", spec, e));
        assert!(out.js.contains("require('./"), "{}: js:\n{}", spec, out.js);
    }
    let src = "---\nimport { state } from 'mist'\nimport x from 'mist/helpers'\nconst n = state(0)\n---\n<span>{n.value}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("no subpaths"), "err: {}", err);
}

#[test]
fn m1004_line_col_stays_accurate_with_ts_annotations() {
    // line 6 col 3 — the interface above and the param annotations must not shift it
    let src = "---\nimport { state } from 'mist'\ninterface Todo { id: number }\nconst items = state<Todo[]>([])\nfunction rm(i: number): void {\n  items.value.splice(i, 1)\n}\n---\n<span>{items.value.length}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1004 at line 6:3"), "err: {}", err);
}

#[test]
fn m1007_fires_inside_template_literal_interpolation() {
    let src = "---\nimport { state } from 'mist'\nconst count = state(0)\n---\n<span>{`Count: ${count}`}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1007"), "err: {}", err);
    let ok = "---\nimport { state } from 'mist'\nconst count = state(0)\n---\n<span>{`Count: ${count.value}`}</span>\n";
    assert!(mistc::compile(ok).is_ok());
}

#[test]
fn m1007_skips_catch_params() {
    let src = "---\nimport { state } from 'mist'\nconst count = state(0)\nfunction f() {\n  try { count.value++ } catch (count) { return count }\n}\n---\n<span>{count.value}</span>\n";
    assert!(mistc::compile(src).is_ok());
}

#[test]
fn non_boolean_config_inline_errors() {
    let src = "---\nimport { state } from 'mist'\nconst n = state(0)\nexport const config = { inline: 'yes' }\n---\n<span>{n.value}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("literal true or false"), "err: {}", err);
}

#[test]
fn m1011_unknown_mist_import_errors() {
    let src = "---\nimport { state, onPulDownRefresh } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1011 at line 2"), "err: {}", err);
    assert!(err.contains("onPulDownRefresh"), "err: {}", err);
    assert!(err.contains("help:"), "err: {}", err);
}

#[test]
fn m1011_aliased_mist_import_errors() {
    let src = "---\nimport { state as s } from 'mist'\nconst n = s(0)\n---\n<span>{n.value}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1011"), "err: {}", err);
    assert!(err.contains("aliased"), "err: {}", err);
}

#[test]
fn m1011_default_mist_import_errors() {
    let src = "---\nimport mist from 'mist'\nconst n = mist.state(0)\n---\n<span>x</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1011"), "err: {}", err);
    assert!(err.contains("named imports"), "err: {}", err);
}

#[test]
fn m1011_fires_in_store_modules() {
    let err = mistc::frontmatter::compile_store_module(
        "import { store, derved } from 'mist'\nexport const s = store(0)\n",
        "./mist-rt.js",
    )
    .unwrap_err();
    assert!(err.contains("M1011"), "err: {}", err);
    assert!(err.contains("derved"), "err: {}", err);
}

#[test]
fn m1012_warns_on_unhandled_pulldown_config() {
    let src = "---\nimport { state } from 'mist'\nconst n = state(0)\nexport const config = { enablePullDownRefresh: true }\n---\n<span>{n.value}</span>\n";
    let unit = mistc::compile_unit(src, true).expect("compile failed");
    assert_eq!(unit.warnings.len(), 1, "warnings: {:?}", unit.warnings);
    assert!(unit.warnings[0].contains("M1012"), "warnings: {:?}", unit.warnings);
    assert!(unit.warnings[0].contains("spinner"), "warnings: {:?}", unit.warnings);
}

#[test]
fn m1012_silent_when_hook_declared_or_disabled() {
    let with_hook = "---\nimport { state, onPullDownRefresh } from 'mist'\nconst n = state(0)\nexport const config = { enablePullDownRefresh: true }\nonPullDownRefresh(() => { n.value = 0 })\n---\n<span>{n.value}</span>\n";
    let unit = mistc::compile_unit(with_hook, true).expect("compile failed");
    assert!(unit.warnings.is_empty(), "warnings: {:?}", unit.warnings);
    let disabled = "---\nimport { state } from 'mist'\nconst n = state(0)\nexport const config = { enablePullDownRefresh: false }\n---\n<span>{n.value}</span>\n";
    let unit = mistc::compile_unit(disabled, true).expect("compile failed");
    assert!(unit.warnings.is_empty(), "warnings: {:?}", unit.warnings);
}

#[test]
fn m1011_allows_inline_type_imports() {
    let src = "---\nimport { state, type Box, type StoreOptions } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n";
    assert!(mistc::compile(src).is_ok());
}

#[test]
fn m1013_page_only_hook_in_component_has_code_and_position() {
    let src = "---\nimport { props, state, onReachBottom } from 'mist'\nconst { label } = props()\nconst n = state(0)\nonReachBottom(() => { n.value++ })\n---\n<span>{label}{n.value}</span>\n";
    let err = match mistc::compile_unit(src, false) {
        Err(e) => e,
        Ok(_) => panic!("expected M1013"),
    };
    assert!(err.contains("M1013 at line 5:1"), "err: {}", err);
    assert!(err.contains("help:"), "err: {}", err);
}

#[test]
fn m1013_app_rejects_non_app_hooks() {
    let dir = std::env::temp_dir().join("mist-m1013-app");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch, onShareAppMessage } from 'mist'\nonLaunch(() => {})\nonShareAppMessage(() => ({ title: 'x' }))\n---\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    let err = match mistc::compile_project_dir(&dir) {
        Err(e) => e,
        Ok(_) => panic!("expected app hook rejection"),
    };
    assert!(err.contains("M1013 at line 4:1"), "err: {}", err);
    assert!(err.contains("onShareAppMessage"), "err: {}", err);
}

#[test]
fn m1007_template_skips_member_access_of_reactive_names() {
    let src = "---\nimport { state } from 'mist'\nconst name = state('')\nconst items = state([])\n---\n<div>{items.value.map(l => (<span key={l.id}>{l.name + name.value}</span>))}</div>\n";
    assert!(mistc::compile(src).is_ok());
    let bare = "---\nimport { state } from 'mist'\nconst name = state('')\n---\n<span>{name}</span>\n";
    assert!(mistc::compile(bare).is_err());
}

#[test]
fn m1013_page_rejects_app_only_hook() {
    let src = "---\nimport { state, onError } from 'mist'\nconst n = state(0)\nonError((error) => { console.log(error) })\n---\n<span>{n.value}</span>\n";
    let err = match mistc::compile_unit(src, true) {
        Err(e) => e,
        Ok(_) => panic!("expected M1013"),
    };
    assert!(err.contains("M1013 at line 4:1"), "err: {}", err);
    assert!(err.contains("app.mist"), "err: {}", err);
}

#[test]
fn m1013_page_rejects_component_only_hook() {
    let src = "---\nimport { state, onCreate } from 'mist'\nconst n = state(0)\nonCreate(() => { console.log(n.value) })\n---\n<span>{n.value}</span>\n";
    let err = match mistc::compile_unit(src, true) {
        Err(e) => e,
        Ok(_) => panic!("expected M1013"),
    };
    assert!(err.contains("M1013 at line 4:1"), "err: {}", err);
    assert!(err.contains("component-only"), "err: {}", err);
}

#[test]
fn m1013_page_rejects_on_attach_and_on_detach() {
    let src = "---\nimport { state, onAttach } from 'mist'\nconst n = state(0)\nonAttach(() => { console.log(n.value) })\n---\n<span>{n.value}</span>\n";
    let err = match mistc::compile_unit(src, true) {
        Err(e) => e,
        Ok(_) => panic!("expected M1013"),
    };
    assert!(err.contains("M1013"), "err: {}", err);
    assert!(err.contains("pages use onLoad"), "err: {}", err);

    let src = "---\nimport { state, onDetach } from 'mist'\nconst n = state(0)\nonDetach(() => { console.log(n.value) })\n---\n<span>{n.value}</span>\n";
    let err = match mistc::compile_unit(src, true) {
        Err(e) => e,
        Ok(_) => panic!("expected M1013"),
    };
    assert!(err.contains("pages use onUnload"), "err: {}", err);
}

#[test]
fn m1013_component_rejects_route_done_and_save_exit_state() {
    let src = "---\nimport { props, onRouteDone } from 'mist'\nconst { label } = props()\nonRouteDone(() => {})\n---\n<span>{label}</span>\n";
    let err = match mistc::compile_unit(src, false) {
        Err(e) => e,
        Ok(_) => panic!("expected M1013"),
    };
    assert!(err.contains("M1013 at line 4:1"), "err: {}", err);
    assert!(err.contains("page-only"), "err: {}", err);
}

#[test]
fn m1013_component_rejects_page_lifecycle_hooks() {
    let cases: &[(&str, &str)] = &[
        ("onShow", "onPageShow"),
        ("onHide", "onPageHide"),
        ("onLoad", "onAttach"),
        ("onUnload", "onDetach"),
    ];
    for (hook, suggestion) in cases {
        let src = format!(
            "---\nimport {{ props, {} }} from 'mist'\nconst {{ label }} = props()\n{}(() => {{}})\n---\n<span>{{label}}</span>\n",
            hook, hook
        );
        let err = match mistc::compile_unit(&src, false) {
            Err(e) => e,
            Ok(_) => panic!("expected M1013 for {}", hook),
        };
        assert!(err.contains("M1013"), "hook {}: err: {}", hook, err);
        assert!(err.contains(suggestion), "hook {}: err: {}", hook, err);
    }
}

#[test]
fn m1017_state_write_in_on_create_errors() {
    let src = "---\nimport { props, state, onCreate } from 'mist'\nconst { label } = props()\nconst n = state(0)\nonCreate(() => { n.value = 1 })\n---\n<span>{label}{n.value}</span>\n";
    let err = match mistc::compile_unit(src, false) {
        Err(e) => e,
        Ok(_) => panic!("expected M1017"),
    };
    assert!(err.contains("M1017"), "err: {}", err);
    assert!(err.contains("onCreate"), "err: {}", err);
}

#[test]
fn m1017_read_only_on_create_compiles() {
    let src = "---\nimport { props, state, onCreate } from 'mist'\nconst { label } = props()\nconst n = state(0)\nonCreate(() => { console.log(n.value) })\n---\n<span>{label}{n.value}</span>\n";
    assert!(mistc::compile_unit(src, false).is_ok());
}

#[test]
fn m1017_catches_mutation_in_nested_closure_inside_on_create() {
    let src = "---\nimport { props, state, onCreate } from 'mist'\nconst { label } = props()\nconst n = state(0)\nonCreate(() => {\n  [1, 2].forEach(() => { n.value = 1 })\n})\n---\n<span>{label}{n.value}</span>\n";
    let err = match mistc::compile_unit(src, false) {
        Err(e) => e,
        Ok(_) => panic!("expected M1017"),
    };
    assert!(err.contains("M1017"), "err: {}", err);
}

#[test]
fn m1017_does_not_false_positive_on_sibling_or_method_mutations() {
    let src = "---\nimport { props, state, onCreate, onAttach } from 'mist'\nconst { label } = props()\nconst n = state(0)\nfunction before() { n.value = 1 }\nonCreate(() => { console.log(n.value) })\nonAttach(() => { n.value = 2 })\nfunction after() { n.value = 3 }\n---\n<span onTap={before}>{label}{n.value}</span>\n";
    let unit = mistc::compile_unit(src, false).expect("compile failed");
    let js = &unit.output.js;
    assert!(js.contains("created() {"), "js:\n{}", js);
    assert!(js.contains("attached() {"), "js:\n{}", js);
}

#[test]
fn m1017_catches_store_mutation_inside_on_create() {
    let dir = std::env::temp_dir().join("mist-m1017-store-write");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("stores.ts"),
        "import { store } from 'mist'\nexport const cart = store({ items: [], total: 0 })\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Widget.mist"),
        "---\nimport { props, onCreate } from 'mist'\nimport { cart } from './stores.ts'\nconst { label } = props()\nonCreate(() => { cart.value.total = 0 })\n---\n<span>{label}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport Widget from '../Widget.mist'\n---\n<Widget label=\"hi\" />\n",
    )
    .unwrap();
    std::fs::write(dir.join("app.mist"), "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n").unwrap();
    let err = match mistc::compile_project_dir(&dir) {
        Err(e) => e,
        Ok(_) => panic!("expected M1017 from store mutation in onCreate"),
    };
    assert!(err.contains("M1017"), "err: {}", err);
}

#[test]
fn m1010_map_rejects_extra_params() {
    let src = "---\nimport { state } from 'mist'\nconst items = state([])\n---\n<div>{items.value.map((a, b, c) => (<span key={a.id}>{a.name}</span>))}</div>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1010 at line 5"), "err: {}", err);
    assert!(err.contains("(item)"), "err: {}", err);
    assert!(err.contains("(item, index)"), "err: {}", err);
}

#[test]
fn m1010_map_rejects_destructured_param() {
    let src = "---\nimport { state } from 'mist'\nconst items = state([])\n---\n<div>{items.value.map(({ id }) => (<span key={id}>{id}</span>))}</div>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1010 at line 5"), "err: {}", err);
    assert!(err.contains("(item)"), "err: {}", err);
    assert!(err.contains("(item, index)"), "err: {}", err);
}

#[test]
fn m1014_app_config_pages_collides_with_generated_field() {
    let dir = std::env::temp_dir().join("mist-m1014-app-pages");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\nexport const config = { pages: ['pages/index/index'] }\n---\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    let err = match mistc::compile_project_dir(&dir) {
        Err(e) => e,
        Ok(_) => panic!("expected M1014"),
    };
    assert!(err.contains("M1014"), "err: {}", err);
    assert!(err.contains("'pages'"), "err: {}", err);
    assert!(err.contains("help:"), "err: {}", err);
}

#[test]
fn m1014_app_config_sitemap_location_collides_with_generated_field() {
    let dir = std::env::temp_dir().join("mist-m1014-app-sitemap");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\nexport const config = { sitemapLocation: 'other.json' }\n---\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<span>{n.value}</span>\n",
    )
    .unwrap();
    let err = match mistc::compile_project_dir(&dir) {
        Err(e) => e,
        Ok(_) => panic!("expected M1014"),
    };
    assert!(err.contains("M1014"), "err: {}", err);
    assert!(err.contains("'sitemapLocation'"), "err: {}", err);
    assert!(err.contains("help:"), "err: {}", err);
}

#[test]
fn m1014_manual_using_components_collides_when_component_is_imported() {
    let dir = std::env::temp_dir().join("mist-m1014-using-components");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::create_dir_all(dir.join("components")).unwrap();
    std::fs::write(
        dir.join("app.mist"),
        "---\nimport { onLaunch } from 'mist'\nonLaunch(() => {})\n---\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("components/badge.mist"),
        "---\nimport { props, state } from 'mist'\nconst { count } = props()\nconst clicks = state(0)\n---\n<span>{count}{clicks.value}</span>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pages/index.mist"),
        "---\nimport Badge from '../components/badge.mist'\nimport { state } from 'mist'\nconst n = state(0)\nexport const config = { usingComponents: { manual: '/components/manual/manual' } }\n---\n<Badge count={n.value} />\n",
    )
    .unwrap();
    let err = match mistc::compile_project_dir(&dir) {
        Err(e) => e,
        Ok(_) => panic!("expected M1014"),
    };
    assert!(err.contains("M1014"), "err: {}", err);
    assert!(err.contains("'usingComponents'"), "err: {}", err);
    assert!(err.contains("help:"), "err: {}", err);
}

#[test]
fn navigate_is_a_valid_mist_value_import() {
    let src = "---\nimport { navigate } from 'mist'\nfunction go() {\n  navigate('/pages/a/a')\n}\n---\n<div onTap={go}>go</div>\n";
    let unit = mistc::compile_unit(src, true).expect("navigate must be a valid mist import");
    assert!(unit.output.js.contains("wx.navigateTo({ url: '/pages/a/a' })"), "js:\n{}", unit.output.js);
}

#[test]
fn navigate_template_literal_without_interpolation_is_accepted_as_literal() {
    let src = "---\nimport { navigate } from 'mist'\nfunction go() {\n  navigate(`/pages/a/a`)\n}\n---\n<div onTap={go}>go</div>\n";
    let unit = mistc::compile_unit(src, true).expect("plain template literal must be treated as a literal route");
    assert!(unit.output.js.contains("wx.navigateTo({ url: '/pages/a/a' })"), "js:\n{}", unit.output.js);
}

#[test]
fn navigate_template_literal_with_interpolation_errors_m1021() {
    let src = "---\nimport { state, navigate } from 'mist'\nconst id = state(3)\nfunction go() {\n  navigate(`/pages/a/${id.value}`)\n}\n---\n<div onTap={go}>go</div>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1021"), "err: {}", err);
    assert!(err.contains("literal strings"), "err: {}", err);
}

#[test]
fn navigate_identifier_route_errors_m1021() {
    let src = "---\nimport { navigate } from 'mist'\nconst r = '/pages/a/a'\nfunction go() {\n  navigate(r)\n}\n---\n<div onTap={go}>go</div>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1021"), "err: {}", err);
    assert!(err.contains("literal strings"), "err: {}", err);
}

#[test]
fn navigate_string_concatenation_errors_m1021() {
    let src = "---\nimport { state, navigate } from 'mist'\nconst id = state(3)\nfunction go() {\n  navigate('/pages/a/' + id.value)\n}\n---\n<div onTap={go}>go</div>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1021"), "err: {}", err);
    assert!(err.contains("literal strings"), "err: {}", err);
}

#[test]
fn navigate_flat_build_compiles_without_route_validation() {
    // flat builds (compile/compile_unit) have no project-wide route set to
    // check against — navigate compiles unconditionally to any literal string.
    let src = "---\nimport { navigate } from 'mist'\nfunction go() {\n  navigate('/pages/does-not-exist/does-not-exist')\n}\n---\n<div onTap={go}>go</div>\n";
    let unit = mistc::compile_unit(src, true).expect("flat builds skip route validation");
    assert!(
        unit.output.js.contains("wx.navigateTo({ url: '/pages/does-not-exist/does-not-exist' })"),
        "js:\n{}",
        unit.output.js
    );
}

#[test]
fn navigate_inside_method_body_rewrites() {
    let src = "---\nimport { navigate } from 'mist'\nfunction go() {\n  navigate('/pages/a/a', { id: 3 })\n}\n---\n<div onTap={go}>go</div>\n";
    let unit = mistc::compile_unit(src, true).unwrap();
    assert!(
        unit.output.js.contains("wx.navigateTo({ url: '/pages/a/a' + __mistq({ id: 3 }) })"),
        "js:\n{}",
        unit.output.js
    );
    assert!(unit.output.js.contains("function __mistq("), "js:\n{}", unit.output.js);
}

#[test]
fn navigate_back_with_and_without_delta() {
    let src = "---\nimport { navigate } from 'mist'\nfunction a() {\n  navigate.back()\n}\nfunction b() {\n  navigate.back(2)\n}\n---\n<div onTap={a}>go</div>\n";
    let unit = mistc::compile_unit(src, true).unwrap();
    assert!(unit.output.js.contains("wx.navigateBack()"), "js:\n{}", unit.output.js);
    assert!(unit.output.js.contains("wx.navigateBack({ delta: 2 })"), "js:\n{}", unit.output.js);
}

#[test]
fn m1022_bound_state_init_calling_frontmatter_function() {
    let src = "---\nimport { state } from 'mist'\nfunction seed() { return [1, 2, 3] }\nconst items = state(seed())\n---\n<span>{items.value.length}</span>\n";
    let err = match mistc::compile_unit(src, true) {
        Err(e) => e,
        Ok(_) => panic!("expected M1022"),
    };
    assert!(err.contains("M1022"), "err: {}", err);
    assert!(err.contains("items"), "err: {}", err);
    assert!(err.contains("module-level const"), "err: {}", err);
}
