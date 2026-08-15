# 测试 mist 应用

[English → testing.md](testing.md)

`mistc test` 编译你的 `src/`，并在一个 Node 测试环境中运行每个
`tests/*.test.js` 文件——用 `Page`/`wx` 桩启动编译后的页面。不需要真机、
不需要开发者工具，除 Node 外没有任何额外依赖。

```sh
mistc test                   # 在项目根目录运行（src/ + tests/）
mistc test --filter cart     # 只运行文件名（不含路径）包含 "cart" 的测试
mistc test --timeout 60      # 单文件超时秒数（默认 30）
```

超过超时的文件会被终止并报告为 `FAIL … timed out`。

每个测试文件就是一个普通的 Node 脚本。导出一个 async 函数；用
`node:assert`（或任何你喜欢的断言方式）：

```js
const assert = require('node:assert');

module.exports = async () => {
  const app = bootPage('index');
  assert.equal(app.data().open.length, 2);

  app.page.toggle(1);
  await flush();
  assert.equal(app.data().open.length, 1);
  assert.ok(app.lastPatch().size < 300);
};
```

导出的函数抛出或 reject 即为失败。`mistc test` 按文件打印
`PASS`/`FAIL`，任何文件失败则以非零退出码结束。

## 测试环境 API

每个测试文件中都有这些全局函数：

- **`bootPage(name, options?)`** —— require 编译后的页面并调用其
  `onLoad`。`name` 是页面短名（`'index'` → `pages/index/index.js`）或
  相对 dist 的路径（`'packages/shop/pages/cart/cart'`）。选项：
  - `query` —— 传给 `onLoad` 的对象（路由查询参数）。
  - `setDataLimit` —— 字节数（默认 1 MB，与微信的真实限制一致）。超大
    patch 在*运行时的批量 flush 内部*抛错，运行时会捕获并回滚——所以
    回滚路径被真实触发，但你的测试看不到这个异常。请改为对句柄的
    `rejected` 数组断言。调低限制可以强制执行更严格的 payload 预算。

  返回一个句柄：
  - `page` —— 注册的 Page 对象：调用你的方法（`app.page.toggle(1)`）、
    读取 `page.data`。
  - `data()` —— `page.data` 的快捷方式。
  - `patches` —— 到目前为止的每次 `setData`：`{ keys, size, patch }`，
    `size` 为字节数。**payload 大小断言正是重点**——路径精确的 toggle
    应该只有几十字节，一条 `size < 300` 的断言能抓住重发整个列表的
    回归。
  - `rejected` —— 因超过 `setDataLimit` 被拒绝的 patch（同样是
    `{ keys, size, patch }` 形状）；它们不会进入 `patches`。注意运行时
    回滚后会用一个小的*成功* patch 重新同步 store 镜像，所以拒绝之后
    `patches` 仍可能增加一条。
  - `lastPatch()` —— 最近一次 patch，或 `null`。
  - `totalBytes()` —— 所有 patch 大小之和。
- **`flush(ms = 0)`** —— 变更后 await 它：运行时在微任务中批量合并
  `setData`，所以要在 `await flush()` 之后再断言。
- **`load(name)`** —— 按相对 dist 的路径 require 任何编译产物
  （例如 `load('stores/cart')` 拿到 store 的实时导出）。
- **`resetModules()`** —— 清空编译文件的模块缓存，让下一次
  `bootPage`/`load` 拿到全新的 store 状态。
- **`appHide()`** —— 触发 `wx.onAppHide` 回调（store 持久化在应用隐藏
  时写回；调用它，然后对 `wx.__storage` 断言）。
- **`wx`** —— 桩：`getStorageSync`/`setStorageSync` 由真实的内存 Map
  （`wx.__storage`）支撑，持久化可以完整往返。其他所有 `wx.*` 调用都是
  被记录的空操作，以 `{ name, args }` 追加到 `wx.__calls`——可对导航、
  toast 等断言。

## 它不是什么

这是一个**逻辑测试环境，不是渲染器**。它在 Node 中运行编译后的页面
JS——状态、派生值、方法、store、持久化、`setData` payload。没有 WXML
渲染、没有组件树、没有事件冒泡，除存储外也没有 `wx` API 的真实行为：
`wx.request` 等都是被记录的空操作，由你断言或自行进一步打桩。只有页面
可以启动：对组件单元调用 `bootPage` 会带解释地失败——请通过使用它的
页面来测试组件逻辑。除非测试自己注册 `App`，否则 `getApp()` 返回
`{}`。像素级/交互测试请用微信开发者工具（可配合
`miniprogram-automator` 驱动）。

`mistc init` 会脚手架出一个可直接运行的 `tests/index.test.js`——从它
开始。
