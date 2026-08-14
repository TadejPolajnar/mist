# API 参考

[English → api.md](api.md)

## `'mist'` 模块

所有可从 `'mist'` 导入的内容。它们都是**编译器内建**——只在编译期存在，并在生成代码前被改写消除；运行时没有 `mist` 这个包。

### `state(initial)` → 盒子

响应式的页面/组件状态。读取 `x.value`；通过 `x.value...` 路径写入。
被模板绑定的状态成为一个 `data` 键；模板从未读取的状态成为实例字段（零桥接成本）。初始值：任意表达式。

### `derived(fn)` → 盒子（只读）

`const open = derived(() => todos.value.filter(t => !t.done))` —— 每个更新批次仅重算一次；依赖即箭头函数读取的那些 state/store 盒子。带模板 `key` 渲染的数组获得键控字段级 diff。不允许在 store 模块中使用。

### `props(defaults?)` → 解构后的 props（仅组件）

`const { todo, onToggle } = props({ todo: null })` —— 必须解构。
普通名字 → `properties`（可序列化的值）。`onXxx` 名字 → 回调 props（子组件调用它们；父组件收到组件事件，接线代码自动生成）。

### `store(initial)` → 共享盒子（仅 store 模块）

在 `stores/*.ts` 中以 `export const cart = store({...})` 声明。读/写契约与 `state` 相同。每个导入它的页面/组件都得到一个订阅镜像，收到路径精确、批量合并的更新。

可选持久化：`store(init, { persist: 'key', version?: 1, migrate? })` 在创建时从 `wx.getStorageSync('key')` 恢复，变更后防抖（约 200 ms）写回，外层包一个 `{ v, data }` 信封。当保存的版本不同时，由 `migrate(oldData, oldVersion)` 生成新的数据结构（没有 `migrate` ⇒ 忽略保存的数据，使用 `init`）。存储错误被吞掉——持久化是尽力而为。注意 wx 存储配额（每键约 1 MB）。
待写入的防抖数据会在 `wx.onAppHide` 时落盘；若在 200 ms 窗口内被强杀，最后一次变更仍可能丢失。

### 生命周期钩子

在 frontmatter 顶层用箭头函数调用：`onLoad(({ id }) => { ... })`。
支持 async 箭头函数。

| 钩子 | 页面 | 组件（映射为） | app.mist |
|---|---|---|---|
| `onLoad` | ✓（注入 init + store 绑定） | — | — |
| `onShow` / `onHide` | ✓ | — | ✓ |
| `onReady` | ✓ | `ready` | — |
| `onUnload` | ✓（注入 store 解绑） | — | — |
| `onAttach` / `onDetach` | — | `attached` / `detached`（注入 init/绑定） | — |
| `onLaunch` | — | — | ✓ |
| `onPullDownRefresh` / `onReachBottom` | ✓ | — | — |
| `onPageScroll` / `onTabItemTap` | ✓ | — | — |
| `onResize` | ✓ | `pageLifetimes.resize` | — |
| `onPageShow` / `onPageHide` | — | `pageLifetimes.show` / `.hide` | — |
| `onShareAppMessage` / `onShareTimeline` / `onAddToFavorites` | ✓（回调的返回值即分享配置；表达式函数体自动返回） | — | — |

### `value:bind`（输入框）

`<input value:bind={text} />` —— 双向绑定：原生 `model:value` 渲染按键输入，没有 setData 回声；生成的 `__vb_text` handler 同步逻辑侧镜像，并通过正常批次重算派生值。

### 提升的表达式

WXML 无法求值的模板绑定——函数调用、模板字符串、可选链——编译为生成的派生值：页面作用域 → `_h<i>` 键；循环内 → 列表被改写为 `_hl<i>`，其条目携带计算出的 `_c<i>` 字段（保留键控 diff）。这些名字是稳定的，在产出的 JS 中可见，便于调试。

### `export const config = { ... }`

静态对象字面量 → 该单元的 `.json`（页面 window 配置，或 `app.mist` 中的应用级 `window` 等）。非字面量的值是编译错误。

## 命令行

```
mistc init <name>
mistc build <src-dir | entry.mist> [-o <outdir>] [--app] [--watch]
```

- **`init`** → 生成 `<name>/`（app.mist、一个待办页面、project.config.json、`mist.d.ts` + `tsconfig.json` + `package.json`，用于编辑器类型提示）。
- **`--watch`** → 每次保存 `.mist`/`.ts` 即重编译（带防抖）。

- **目录** → 项目构建。需要 `<dir>/app.mist` 和 `<dir>/pages/*.mist`（index 是启动页）。组件和 store 通过导入发现。输出使用微信目录布局。
- **单文件** → 平铺构建一个页面及其导入；`--app` 附带一个最小可打开的应用壳（`App({})`、`app.json`、游客 appid 配置）。
- `-o` 默认为 `dist`。错误以退出码 1 结束并带 `M` 编码消息；`M1002`/`M1006` 是非致命的 stderr 警告。

## 产物输出（项目构建）

```
dist/
├── app.js  app.json  app.wxss  sitemap.json
├── mist-rt.js                  # the runtime (~9.6 KB)
├── tw-shared.wxss              # tailwind utilities (imported by every unit)
├── tw-theme.wxss               # page{} theme vars (imported by pages only)
├── pages/<name>/<name>.{js,wxml,wxss,json}
├── components/<kebab>/<kebab>.{js,wxml,wxss,json}
│                               # pure-render components: .wxml template only
└── stores/<name>.js
```

生成的 JS 刻意保持可读（微信开发者工具无法加载 source map）：普通的 `Page({...})`/`Component({...})` 对象，保留你的命名。

## 运行时（`mist-rt.js`）

你永远不需要导入它——生成的代码会导入。为了调试，其接口如下：

- `set(page, path, value)` / `touch(page)` / `flush(page)` —— 批量合并一次写入 / 一次仅重算派生值的遍历 / 每微任务一次的刷写（flush），只发出一次 `setData`。
  被拒绝的 `setData`（例如载荷过大）会把页面的本地镜像回滚，随后绑定了 store 的页面会从当前 store 值重新播种镜像——失败的批次绝不会让页面与其 store 失去同步。
- `init(page)` —— 首次渲染的派生值播种（由生成的 `onLoad`/`attached` 调用）。
- `applyPath(obj, path, value)` —— 批处理器使用的路径字符串写入器。
- `derive(page, out, name, key, compute, deps)` —— 重算一个派生值，对快照做键控字段级 diff；`deps` 驱动按派生值粒度的脏位跳过（null ⇒ 总是重算）。
- `store(init)`、`bindStores`、`unbindStores` —— 共享状态盒子与页面订阅胶水代码。
- `observePerf()` / `perfEntries` —— 由生成的 `app.js` 安装的 `wx.getPerformance` 观察器；条目可通过 `getApp().__perf` 读取。

## 为你自己的应用做基准测试

`benchmark/devtools/measure.js` 可用于任何已构建的小程序（需在微信开发者工具中启用 Service Port）：

```sh
node measure.js <project-path> [row-selector]   # selector 'host|.inner' pierces components
TOGGLES=100 PAGE=/pages/x/x node measure.js my-app '.my-row'
```

报告 setData 次数/字节数、点击延迟 p50/p95、初始数据体积、过滤操作开销、启动条目、包体积。
