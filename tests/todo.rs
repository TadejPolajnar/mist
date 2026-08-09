#[test]
fn compiles_todo_example() {
    let src = include_str!("../examples/todo.mist");
    let out = mistc::compile(src).expect("compile failed");

    // WXML: tags mapped, bindings stripped of .value, keyed loop, conditional
    assert!(out.wxml.contains("<view class=\"p-4\">"), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("{{visible.length}}"), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("wx:for=\"{{visible}}\""), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("wx:for-item=\"t\""), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("wx:key=\"id\""), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("wx:if=\"{{visible.length === 0}}\""), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("bindtap=\"_e0\""), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("data-a0=\"{{t.id}}\""), "wxml:\n{}", out.wxml);
    assert!(out.wxml.contains("bindtap=\"switchFilter\""), "wxml:\n{}", out.wxml);

    // JS: `todos` is unbound (template renders `visible`) → dead-data elimination:
    // instance-field mutation + rt.touch, and todos stays out of data entirely
    assert!(out.js.contains("(this._todos[i].done = !this._todos[i].done, rt.touch(this, 'todos'))"), "js:\n{}", out.js);
    assert!(out.js.contains("this._todos = ["), "js:\n{}", out.js);
    assert!(!out.js.contains("todos: ["), "js:\n{}", out.js);
    assert!(out.js.contains("this.__set('filter',"), "js:\n{}", out.js);
    assert!(out.js.contains("this.data.filter === 'all'"), "js:\n{}", out.js);
    assert!(out.js.contains("rt.derive(this, __o, 'visible', 'id',"), "js:\n{}", out.js);
    assert!(out.js.contains("_e0(e)"), "js:\n{}", out.js);
    assert!(out.js.contains("this.toggle(e.currentTarget.dataset.a0)"), "js:\n{}", out.js);
    assert!(out.js.contains("onShow()"), "js:\n{}", out.js);

    // config → json
    let json = out.json.expect("missing json");
    assert!(json.contains("\"navigationBarTitleText\": \"Todos\""), "json:\n{}", json);
}
