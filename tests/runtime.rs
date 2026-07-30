use std::process::Command;

fn run_node(script: &str) -> String {
    let rt_path = concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/mist-rt.js");
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
