use std::process::Command;

fn run_node(script: &str) -> String {
    let rt_path = concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/mist-rt.js").replace('\\', "/");
    let full = format!("const rt = require('{}');\n{}", rt_path, script);
    let out = Command::new("node")
        .arg("-e")
        .arg(&full)
        .output()
        .expect("node not found — runtime tests require Node.js");
    assert!(
        out.status.success(),
        "node failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn make_page(js_body: &str) -> String {
    format!(
        r#"
const calls = [];
const page = {{
  data: {{ todos: [{{ id: 1, done: false }}, {{ id: 2, done: true }}], visible: null }},
  setData(o) {{ calls.push(o); }},
  __derive() {{
    const __o = {{}};
    rt.derive(this, __o, 'visible', 'id', () => this.data.todos.filter(() => true));
    return __o;
  }},
  __set(p, v) {{ rt.set(this, p, v); }},
}};
rt.init(page);
{}
"#,
        js_body
    )
}

#[test]
fn init_writes_full_derived_array() {
    let out = run_node(&make_page(
        r#"
if (calls.length !== 1) throw new Error('expected 1 call');
if (!Array.isArray(calls[0].visible) || calls[0].visible.length !== 2) throw new Error('bad init write');
console.log('OK');
"#,
    ));
    assert_eq!(out, "OK");
}

#[test]
fn mutations_batch_into_one_setdata_with_item_level_diff() {
    let out = run_node(&make_page(
        r#"
page.__set('todos[0].done', true);
page.__set('todos[1].done', false);
setTimeout(() => {
  if (calls.length !== 2) throw new Error('batching failed: ' + calls.length + ' calls');
  const c = calls[1];
  const keys = Object.keys(c).sort();
  const expect = ['todos[0].done', 'todos[1].done', 'visible[0].done', 'visible[1].done'];
  if (JSON.stringify(keys) !== JSON.stringify(expect)) {
    throw new Error('wrong keys: ' + JSON.stringify(keys));
  }
  if ('visible' in c) throw new Error('full array resent');
  console.log('OK');
}, 10);
"#,
    ));
    assert_eq!(out, "OK");
}

#[test]
fn unchanged_derived_is_not_resent() {
    let out = run_node(&make_page(
        r#"
page.__set('unrelated', 42);
setTimeout(() => {
  const c = calls[1];
  const keys = Object.keys(c);
  if (keys.length !== 1 || keys[0] !== 'unrelated') {
    throw new Error('derived resent without change: ' + JSON.stringify(keys));
  }
  console.log('OK');
}, 10);
"#,
    ));
    assert_eq!(out, "OK");
}

#[test]
fn length_change_falls_back_to_full_write() {
    let out = run_node(&make_page(
        r#"
page.__set('todos', [{ id: 3, done: false }]);
setTimeout(() => {
  const c = calls[1];
  if (!Array.isArray(c.visible) || c.visible.length !== 1) {
    throw new Error('expected full visible write: ' + JSON.stringify(c));
  }
  console.log('OK');
}, 10);
"#,
    ));
    assert_eq!(out, "OK");
}

#[test]
fn store_notifies_pages_with_path_precise_writes() {
    let out = run_node(
        r#"
const s = rt.store({ taps: 0, list: [] });
const calls = [];
const page = {
  data: { stats: null },
  setData(o) { calls.push(o); },
  __derive() { return {}; },
};
rt.bindStores(page, [[s, 'stats']]);
if (JSON.stringify(calls[0]) !== JSON.stringify({ stats: { taps: 0, list: [] } })) throw new Error('seed failed: ' + JSON.stringify(calls[0]));

s.__set('taps', 1);
s.__set(`list[${s.value.list.length}]`, 'x');
setTimeout(() => {
  const c = calls[1];
  if (c['stats.taps'] !== 1) throw new Error('path write missing: ' + JSON.stringify(c));
  if (c['stats.list[0]'] !== 'x') throw new Error('index write missing: ' + JSON.stringify(c));
  if (s.value.taps !== 1 || s.value.list[0] !== 'x') throw new Error('store value not updated');

  rt.unbindStores(page);
  s.__set('taps', 2);
  setTimeout(() => {
    if (calls.length !== 2) throw new Error('unsubscribed page still notified');
    console.log('OK');
  }, 5);
}, 5);
"#,
    );
    assert_eq!(out, "OK");
}

#[test]
fn store_whole_value_replacement() {
    let out = run_node(
        r#"
const s = rt.store({ a: 1 });
const calls = [];
const page = { data: {}, setData(o) { calls.push(o); }, __derive() { return {}; } };
rt.bindStores(page, [[s, 'st']]);
s.__set(null, { a: 9 });
setTimeout(() => {
  if (JSON.stringify(calls[1]) !== JSON.stringify({ st: { a: 9 } })) throw new Error(JSON.stringify(calls));
  if (s.value.a !== 9) throw new Error('value not replaced');
  console.log('OK');
}, 5);
"#,
    );
    assert_eq!(out, "OK");
}

#[test]
fn two_pages_share_one_store() {
    let out = run_node(
        r#"
const s = rt.store({ n: 0 });
const mk = () => { const calls = []; return { calls, page: { data: {}, setData(o) { calls.push(o); }, __derive() { return {}; } } }; };
const a = mk(), b = mk();
rt.bindStores(a.page, [[s, 'st']]);
rt.bindStores(b.page, [[s, 'st']]);
s.__set('n', 5);
setTimeout(() => {
  if (a.calls[1]['st.n'] !== 5) throw new Error('page A not notified');
  if (b.calls[1]['st.n'] !== 5) throw new Error('page B not notified');
  console.log('OK');
}, 5);
"#,
    );
    assert_eq!(out, "OK");
}

#[test]
fn touch_flushes_derive_only_for_unbound_state() {
    let out = run_node(
        r#"
const calls = [];
const page = {
  data: { visible: null },
  setData(o) { calls.push(o); },
  __derive() {
    const __o = {};
    rt.derive(this, __o, 'visible', 'id', () => this._todos);
    return __o;
  },
};
page._todos = [{ id: 1, done: false }];
rt.init(page);
(page._todos[0].done = true, rt.touch(page));
setTimeout(() => {
  if (calls.length !== 2) throw new Error('expected 2 calls: ' + calls.length);
  if (JSON.stringify(calls[1]) !== JSON.stringify({ 'visible[0].done': true })) {
    throw new Error('bad payload: ' + JSON.stringify(calls[1]));
  }
  console.log('OK');
}, 10);
"#,
    );
    assert_eq!(out, "OK");
}

#[test]
fn apply_path_handles_nested_segments() {
    let out = run_node(
        r#"
const data = { a: { b: [{ c: 1 }] } };
rt.applyPath(data, 'a.b[0].c', 9);
rt.applyPath(data, 'a.b[2]', { c: 3 });
rt.applyPath(data, 'fresh.x', 1);
if (data.a.b[0].c !== 9) throw new Error('nested write failed');
if (data.a.b[2].c !== 3) throw new Error('index write failed');
if (data.fresh.x !== 1) throw new Error('create-on-write failed');
console.log('OK');
"#,
    );
    assert_eq!(out, "OK");
}

#[test]
fn dirty_deps_skip_unrelated_deriveds() {
    let out = run_node(
        r#"
const calls = [];
let aRuns = 0, bRuns = 0;
const page = {
  data: { x: 1, y: 10, a: null, b: null },
  setData(o) { calls.push(o); },
  __derive() {
    const __o = {};
    rt.derive(this, __o, 'a', null, () => { aRuns++; return this.data.x * 2; }, ['x']);
    rt.derive(this, __o, 'b', null, () => { bRuns++; return this.data.y * 2; }, ['y']);
    return __o;
  },
  __set(p, v) { rt.set(this, p, v); },
};
rt.init(page);
page.__set('x', 5);
setTimeout(() => {
  if (aRuns !== 2) throw new Error('a should recompute: ' + aRuns);
  if (bRuns !== 1) throw new Error('b should be skipped: ' + bRuns);
  if (calls[1].a !== 10) throw new Error('bad a: ' + calls[1].a);
  if ('b' in calls[1]) throw new Error('b should not be written');
  console.log('OK');
}, 10);
"#,
    );
    assert_eq!(out, "OK");
}

#[test]
fn dirty_chain_recomputes_dependent_derived() {
    let out = run_node(
        r#"
let bRuns = 0;
const calls = [];
const page = {
  data: { x: 1, a: null, b: null },
  setData(o) { calls.push(o); },
  __derive() {
    const __o = {};
    rt.derive(this, __o, 'a', null, () => this.data.x + 1, ['x']);
    rt.derive(this, __o, 'b', null, () => { bRuns++; return this.data.a * 10; }, ['a']);
    return __o;
  },
  __set(p, v) { rt.set(this, p, v); },
};
rt.init(page);
page.__set('x', 4);
setTimeout(() => {
  if (bRuns !== 2) throw new Error('b must follow a: ' + bRuns);
  if (calls[1].b !== 50) throw new Error('bad chained b: ' + calls[1].b);
  console.log('OK');
}, 10);
"#,
    );
    assert_eq!(out, "OK");
}

#[test]
fn nameless_touch_recomputes_everything() {
    let out = run_node(
        r#"
let aRuns = 0, bRuns = 0;
const page = {
  data: { x: 1, y: 1, a: null, b: null },
  setData() {},
  __derive() {
    const __o = {};
    rt.derive(this, __o, 'a', null, () => { aRuns++; return this.data.x; }, ['x']);
    rt.derive(this, __o, 'b', null, () => { bRuns++; return this.data.y; }, ['y']);
    return __o;
  },
  __set(p, v) { rt.set(this, p, v); },
};
rt.init(page);
rt.touch(page);
setTimeout(() => {
  if (aRuns !== 2 || bRuns !== 2) throw new Error('nameless touch must recompute all: ' + aRuns + ',' + bRuns);
  console.log('OK');
}, 10);
"#,
    );
    assert_eq!(out, "OK");
}

#[test]
fn rejected_setdata_rolls_back_and_retries() {
    let out = run_node(
        r#"
let n = 0;
const calls = [];
const page = {
  data: { todos: [{ id: 1, done: false }], visible: null },
  setData(o) {
    n++;
    if (n === 2) throw new Error('data too large');
    if (n > 1) calls.push(o);
  },
  __derive() {
    const __o = {};
    rt.derive(this, __o, 'visible', 'id', () => this.data.todos.filter(() => true), ['todos']);
    return __o;
  },
  __set(p, v) { rt.set(this, p, v); },
};
rt.init(page);
page.__set('todos[0].done', true);
setTimeout(() => {
  if (page.data.todos[0].done !== false) throw new Error('mirror not rolled back');
  if (page.__prev.visible[0].done !== false) throw new Error('__prev advanced despite rejection');
  page.__set('todos[0].done', true);
  setTimeout(() => {
    if (calls.length !== 1) throw new Error('retry flush missing: ' + calls.length);
    if (calls[0]['todos[0].done'] !== true) throw new Error('retry payload missing state write');
    if (calls[0]['visible[0].done'] !== true) throw new Error('retry payload missing derived write: ' + JSON.stringify(calls[0]));
    console.log('OK');
  }, 10);
}, 10);
"#,
    );
    assert_eq!(out, "OK");
}

#[test]
fn rejected_setdata_resyncs_store_mirror_from_store_truth() {
    let out = run_node(
        r#"
const store = rt.store({ n: 1 });
let fail = false;
const calls = [];
const page = {
  data: { st: null },
  setData(o) {
    if (fail) { fail = false; throw new Error('data too large'); }
    calls.push(o);
  },
  __derive() { return {}; },
  __set(p, v) { rt.set(this, p, v); },
};
rt.bindStores(page, [[store, 'st']]);
fail = true;
store.__set('n', 5);
setTimeout(() => {
  if (store.value.n !== 5) throw new Error('store must keep the committed value');
  if (page.data.st.n !== 5) throw new Error('mirror must resync to store truth: ' + JSON.stringify(page.data.st));
  const last = calls[calls.length - 1];
  if (!last || !last.st || last.st.n !== 5) throw new Error('resync setData missing: ' + JSON.stringify(calls));
  console.log('OK');
}, 20);
"#,
    );
    assert_eq!(out, "OK");
}

#[test]
fn unchanged_keyed_rows_reuse_prev_snapshots() {
    let out = run_node(
        r#"
const page = {
  data: { todos: [{ id: 1, done: false }, { id: 2, done: false }], visible: null },
  setData() {},
  __derive() {
    const __o = {};
    rt.derive(this, __o, 'visible', 'id', () => this.data.todos.map(t => ({ id: t.id, done: t.done })), ['todos']);
    return __o;
  },
  __set(p, v) { rt.set(this, p, v); },
};
rt.init(page);
const snap0 = page.__prev.visible[1];
page.__set('todos[0].done', true);
setTimeout(() => {
  if (page.__prev.visible[1] !== snap0) throw new Error('unchanged row snapshot was reallocated');
  if (page.__prev.visible[0].done !== true) throw new Error('changed row snapshot stale');
  console.log('OK');
}, 10);
"#,
    );
    assert_eq!(out, "OK");
}

#[test]
fn rejected_push_rolls_back_array_length() {
    let out = run_node(
        r#"
let n = 0;
const page = {
  data: { todos: [{ id: 1 }], visible: null },
  setData() {
    n++;
    if (n === 2) throw new Error('data too large');
  },
  __derive() {
    const __o = {};
    rt.derive(this, __o, 'visible', 'id', () => this.data.todos.filter(() => true), ['todos']);
    return __o;
  },
  __set(p, v) { rt.set(this, p, v); },
};
rt.init(page);
page.__set('todos[' + page.data.todos.length + ']', { id: 2 });
setTimeout(() => {
  if (page.data.todos.length !== 1) throw new Error('array length not rolled back: ' + page.data.todos.length);
  if (page.data.todos.some((t) => t === undefined)) throw new Error('undefined hole left behind');
  console.log('OK');
}, 10);
"#,
    );
    assert_eq!(out, "OK");
}

#[test]
fn persisted_store_hydrates_and_writes_debounced() {
    let out = run_node(
        r#"
const disk = {};
let writes = 0;
global.wx = {
  getStorageSync(k) { return disk[k]; },
  setStorageSync(k, v) { writes++; disk[k] = v; },
};
disk.cart = { v: 1, data: { items: ['seeded'], total: 5 } };
const s = rt.store({ items: [], total: 0 }, { persist: 'cart', version: 1 });
if (s.value.items[0] !== 'seeded') throw new Error('hydration failed: ' + JSON.stringify(s.value));
s.__set('total', 6);
s.__set('total', 7);
setTimeout(() => {
  if (writes !== 1) throw new Error('expected 1 debounced write, got ' + writes);
  if (disk.cart.data.total !== 7) throw new Error('persisted stale value: ' + disk.cart.data.total);
  if (disk.cart.v !== 1) throw new Error('version envelope missing');
  console.log('OK');
}, 350);
"#,
    );
    assert_eq!(out, "OK");
}

#[test]
fn persisted_store_migrates_old_versions() {
    let out = run_node(
        r#"
const disk = { s: { v: 1, data: { n: 3 } } };
global.wx = {
  getStorageSync(k) { return disk[k]; },
  setStorageSync(k, v) { disk[k] = v; },
};
const s = rt.store({ n: 0, label: '' }, {
  persist: 's',
  version: 2,
  migrate(old, oldV) { return { n: old.n, label: 'migrated-from-' + oldV }; },
});
if (s.value.label !== 'migrated-from-1') throw new Error('migrate not applied: ' + JSON.stringify(s.value));
if (s.value.n !== 3) throw new Error('migrate lost data');
console.log('OK');
"#,
    );
    assert_eq!(out, "OK");
}

#[test]
fn unpersisted_store_never_touches_storage() {
    let out = run_node(
        r#"
let touched = 0;
global.wx = {
  getStorageSync() { touched++; },
  setStorageSync() { touched++; },
};
const s = rt.store({ n: 0 });
s.__set('n', 1);
setTimeout(() => {
  if (touched !== 0) throw new Error('storage touched ' + touched + ' times');
  if (s.__persistTimer) throw new Error('timer scheduled without persist');
  console.log('OK');
}, 250);
"#,
    );
    assert_eq!(out, "OK");
}

#[test]
fn migrate_persists_immediately_and_undefined_falls_back_to_init() {
    let out = run_node(
        r#"
const disk = { s: { v: 1, data: { n: 3 } }, u: { v: 1, data: { n: 9 } } };
global.wx = {
  getStorageSync(k) { return disk[k]; },
  setStorageSync(k, v) { disk[k] = v; },
};
const s = rt.store({ n: 0 }, { persist: 's', version: 2, migrate(old) { return { n: old.n * 10 }; } });
if (s.value.n !== 30) throw new Error('migrate not applied');
if (disk.s.v !== 2 || disk.s.data.n !== 30) throw new Error('migrated envelope not written back: ' + JSON.stringify(disk.s));
const u = rt.store({ n: 1 }, { persist: 'u', version: 2, migrate() {} });
if (u.value.n !== 1) throw new Error('undefined migrate must fall back to init: ' + JSON.stringify(u.value));
console.log('OK');
"#,
    );
    assert_eq!(out, "OK");
}

#[test]
fn over_budget_batches_chunk_in_order() {
    let out = run_node(
        r#"
rt.setDataBudget(200);
const calls = [];
const page = {
  data: {},
  setData(o) { calls.push(Object.keys(o)); },
  __derive() { return {}; },
};
const big = 'x'.repeat(80);
for (let i = 0; i < 6; i++) {
  rt.set(page, 'k' + i, big);
}
rt.flush(page);
if (calls.length < 2) throw new Error('expected chunked calls, got ' + calls.length);
const seen = calls.flat();
if (seen.length !== 6) throw new Error('keys lost: ' + JSON.stringify(calls));
for (let i = 0; i < 6; i++) {
  if (seen[i] !== 'k' + i) throw new Error('order broken: ' + JSON.stringify(seen));
}
if (page.data.k0 !== big || page.data.k5 !== big) throw new Error('mirror not applied');
console.log('CHUNKED ' + calls.length);
"#,
    );
    assert!(out.starts_with("CHUNKED"), "out: {}", out);
}

#[test]
fn under_budget_batches_stay_single_call() {
    let out = run_node(
        r#"
const calls = [];
const page = {
  data: {},
  setData(o) { calls.push(Object.keys(o).length); },
  __derive() { return {}; },
};
rt.set(page, 'a', 1);
rt.set(page, 'b', 2);
rt.flush(page);
if (calls.length !== 1 || calls[0] !== 2) throw new Error('expected one call: ' + JSON.stringify(calls));
console.log('SINGLE');
"#,
    );
    assert_eq!(out, "SINGLE");
}

#[test]
fn oversized_single_key_is_never_split() {
    let out = run_node(
        r#"
rt.setDataBudget(200);
const calls = [];
const page = {
  data: {},
  setData(o) {
    calls.push(Object.keys(o).length);
    throw new Error('too big');
  },
  __derive() { return {}; },
};
rt.set(page, 'small', 1);
rt.set(page, 'huge', 'x'.repeat(500));
rt.flush(page);
if (calls.length !== 1) throw new Error('oversized entry must keep one whole-batch call: ' + JSON.stringify(calls));
if (calls[0] !== 2) throw new Error('whole batch must go in the single call');
if ('small' in page.data || 'huge' in page.data) throw new Error('rollback must restore the mirror');
console.log('WHOLE-REJECTED');
"#,
    );
    assert_eq!(out, "WHOLE-REJECTED");
}

#[test]
fn chunk_sizing_counts_utf8_bytes_not_utf16_units() {
    let out = run_node(
        r#"
rt.setDataBudget(260);
const calls = [];
const page = {
  data: {},
  setData(o) { calls.push(Object.keys(o)); },
  __derive() { return {}; },
};
const cjk = '雾'.repeat(60);
rt.set(page, 'a', cjk);
rt.set(page, 'b', cjk);
rt.flush(page);
if (calls.length !== 2) throw new Error('CJK payload must size by bytes and chunk: ' + calls.length);
console.log('BYTES');
"#,
    );
    assert_eq!(out, "BYTES");
}

#[test]
fn chunk_sizing_never_undercounts_lone_surrogates() {
    let out = run_node(
        r#"
rt.setDataBudget(40);
const calls = [];
const page = {
  data: {},
  setData(o) { calls.push(Object.keys(o)); },
  __derive() { return {}; },
};
rt.set(page, 'a', '\ud800€€€');
rt.set(page, 'b', '\ud800€€€');
rt.flush(page);
if (calls.length !== 2) throw new Error('lone-surrogate strings must still size safely: ' + calls.length);
console.log('SAFE');
"#,
    );
    assert_eq!(out, "SAFE");
}

#[test]
fn nested_hoist_rows_skip_unchanged_and_rewrite_changed() {
    let out = run_node(
        r#"
const calls = [];
const page = {
  data: { groups: [{ items: [{ id: 1, ts: 0 }] }, { items: [{ id: 2, ts: 0 }] }], _hl0: null },
  setData(o) { calls.push(o); },
  __derive() {
    const __o = {};
    rt.derive(this, __o, '_hl0', 'id', () => this.data.groups.map((g, index) => g.items.map(it => ({ ...it, _c0: 'v' + it.ts }))), ['groups']);
    return __o;
  },
  __set(p, v) { rt.set(this, p, v); },
};
rt.init(page);
page.__set('groups[1].items[0].ts', 9);
setTimeout(() => {
  const last = calls[calls.length - 1];
  const keys = Object.keys(last);
  if (keys.some(k => k === '_hl0' || k === '_hl0[0]')) throw new Error('unchanged row rewritten: ' + JSON.stringify(last));
  if (!last['_hl0[1]'] || last['_hl0[1]'][0]._c0 !== 'v9') throw new Error('changed row not rewritten: ' + JSON.stringify(last));
  console.log('OK');
}, 0);
"#,
    );
    assert_eq!(out, "OK");
}

#[test]
fn trace_logs_patches_when_enabled() {
    let out = run_node(
        r#"
const logs = [];
const origLog = console.log;
console.log = (...a) => logs.push(a.join(' '));
rt.trace(true);
const calls = [];
const page = {
  data: { count: 0 },
  setData(o) { calls.push(o); },
  __derive() { return {}; },
  __set(p, v) { rt.set(this, p, v); },
};
rt.init(page);
page.__set('count', 1);
setTimeout(() => {
  console.log = origLog;
  const mistLogs = logs.filter(l => l.startsWith('[mist]'));
  if (mistLogs.length !== 1) throw new Error('expected 1 mist log, got ' + mistLogs.length);
  if (!mistLogs[0].includes('count')) throw new Error('log missing key: ' + mistLogs[0]);
  console.log('OK');
}, 0);
"#,
    );
    assert_eq!(out, "OK");
}

#[test]
fn trace_off_by_default_logs_nothing() {
    let out = run_node(
        r#"
const logs = [];
const origLog = console.log;
console.log = (...a) => logs.push(a.join(' '));
const calls = [];
const page = {
  data: { count: 0 },
  setData(o) { calls.push(o); },
  __derive() { return {}; },
  __set(p, v) { rt.set(this, p, v); },
};
rt.init(page);
page.__set('count', 1);
setTimeout(() => {
  console.log = origLog;
  const mistLogs = logs.filter(l => l.startsWith('[mist]'));
  if (mistLogs.length !== 0) throw new Error('expected 0 mist logs, got ' + mistLogs.length);
  console.log('OK');
}, 0);
"#,
    );
    assert_eq!(out, "OK");
}

#[test]
fn rollback_restores_null_rooted_vivified_paths() {
    let out = run_node(
        r#"
const page = {
  data: { obj: null, big: 'x' },
  setData(o) { if ('big' in o) throw new Error('reject'); },
  __derive() { return {}; },
  __set(p, v) { rt.set(this, p, v); },
};
rt.init(page);
page.__set('obj.field', 1);
page.__set('big', 'y'.repeat(10));
setTimeout(() => {
  if (page.data.obj !== null) throw new Error('vivified root not restored: ' + JSON.stringify(page.data.obj));
  if (page.data.big !== 'x') throw new Error('leaf not rolled back: ' + page.data.big);
  console.log('OK');
}, 20);
"#,
    );
    assert_eq!(out, "OK");
}
