<p align="center">
  <img src="docs/assets/cover.png" width="620" alt="Mist — Mini-app Static Templates" />
</p>

<p align="center">
  <b>面向微信小程序的组件语言与编译器——可以理解为「小程序界的 Svelte」。</b><br />
  Astro 风格的单文件组件，由 Rust 编译为原生小程序代码，性能接近手写。
</p>

<p align="center">
  <a href="https://github.com/TadejPolajnar/mist/actions/workflows/ci.yml"><img src="https://github.com/TadejPolajnar/mist/actions/workflows/ci.yml/badge.svg" alt="ci" /></a>
  <a href="https://www.npmjs.com/package/mist-lang"><img src="https://img.shields.io/npm/v/mist-lang?color=07c160&label=mist-lang" alt="npm" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT" /></a>
</p>

<p align="center">
  <a href="docs/README.zh-CN.md">快速上手</a> ·
  <a href="docs/language.zh-CN.md">语言指南</a> ·
  <a href="docs/api.zh-CN.md">API</a> ·
  <a href="docs/testing.zh-CN.md">测试</a> ·
  <a href="docs/diagnostics.zh-CN.md">诊断说明</a> ·
  <a href="README.md">English</a>
</p>

---

## 安装

```sh
npm install -g mist-lang     # macOS / Linux / Windows 预编译二进制
mistc --version
```

也可以从源码安装：在仓库克隆中执行 `cargo install --path .`（Rust 2021）。
两种方式都还需要 PATH 中的 **Node.js + npm**（Tailwind 通过真实的
`@tailwindcss/cli` 运行），以及**微信开发者工具**来运行产物。

```sh
mistc init my-app            # 脚手架：app.mist + todo 页面 + 开发者工具配置
cd my-app
mistc build src --watch      # 保存即重编译 · 在微信开发者工具中导入 my-app/
```

编写 `.mist` 单文件组件（TypeScript frontmatter + 类 JSX 模板 + Tailwind），得到普通的 `Page()`/`Component()` 小程序代码，核心是**路径精确的 `setData`**：编译器静态追踪每一次状态变更，并生成它所改变的精确数据路径。没有虚拟 DOM，没有运行时树 diff，运行时仅约 10 KB（gzip 后 3.2 KB）。

```
┌──────────────┐     mistc (Rust)      ┌──────────────────────────────┐
│  .mist 文件   │ ───────────────────► │  WXML + WXSS + JS + JSON     │
│  stores/*.ts  │   oxc · tailwind v4  │  （微信开发者工具可直接打开）    │
└──────────────┘                       └──────────────────────────────┘
```

**实测数据**（[benchmark/](benchmark/)）：在 1000 行的过滤列表中切换一行，经 setData 桥只发送 **26 字节**——与手写 setData 的下限只差一个很小的常数（由 `tests/bench.rs` 守护）——且恰好合并为一次调用，比朴素的整表重发少约 2000 倍（Node 环境：49 B vs 96.6 KB）。在真实微信开发者工具中与 Taro 3 + React 对比（[benchmark/devtools/](benchmark/devtools/)）：**单次交互约快 2.4 倍**、**每次切换的桥接流量少 2.6 倍**、**包体积小 29 倍**（原始 10.7 KB vs 309.8 KB；gzip 后 4.3 KB vs 86.9 KB——mist 未压缩，Taro 为 webpack 生产构建）。

## 尝一口

```jsx
---
import TodoItem from '../components/TodoItem.mist'
import { stats, track } from '../stores/stats.ts'
import { state, derived } from 'mist'

export const config = { navigationBarTitleText: 'Todos' }

const filter = state('all')
const todos = state([{ id: 1, title: '发布它', done: false }])

const visible = derived(() =>
  filter.value === 'all' ? todos.value : todos.value.filter(t => !t.done)
)

function toggle(id) {
  const i = todos.value.findIndex(t => t.id === id)
  todos.value[i].done = !todos.value[i].done   // → setData({`todos[${i}].done`: …})
  track('toggle')                              // 共享 store，所有打开的页面同步更新
}
---
<div class="p-4 flex flex-col gap-2">
  <span class="text-2xl font-bold text-blue-600">待办（{visible.value.length}）</span>
  {visible.value.map(t => (
    <TodoItem key={t.id} todo={t} onToggle={toggle} />
  ))}
  {visible.value.length === 0 && <span class="text-gray-400">这里空空如也</span>}
  <button class="rounded-full bg-blue-500 text-white" onTap={() => filter.value = 'open'}>
    只看未完成
  </button>
</div>
```

## 示例应用

| [雾茶 · 点单](examples/food) | [雾投 · 行情](examples/portfolio) | [雾板 · 看板](examples/kanban) |
|:---:|:---:|:---:|
| <img src="examples/food/screenshot.png" width="220" alt="雾茶" /> | <img src="examples/portfolio/screenshot.png" width="220" alt="雾投" /> | <img src="examples/kanban/screenshot.png" width="220" alt="雾板" /> |
| 持久化购物车与订单、分包结算页、tab 图标、`migrate` 迁移 | 13 节点派生图、键控 diff、确定性行情 | 键控排序、跨 store 派生、在制限制 |

每个示例都带 README、门禁测试套件和可直接导入开发者工具的 `project.config.json`。

## 参与编译器开发

```sh
git clone https://github.com/TadejPolajnar/mist.git && cd mist
cargo run -- build examples/project/src -o dist   # 编译最小示例
# 微信开发者工具 → 导入项目 → 选择本仓库根目录（miniprogramRoot: dist/）

cargo test              # 完整测试套件（会调用 node 和 npm）
node benchmark/bench.js # 桥接流量基准测试
cargo install --path crates/mistc-lsp   # 编辑器 LSP（配合 editors/vscode）
```

### 命令行

```
mistc init <name>                                        # 脚手架新项目
mistc build <src目录 | 入口.mist> [-o <输出目录>] [--app] [--watch]
```

- **`init`** → 生成 `<name>/`：`src/app.mist`、一个待办页面、`project.config.json`（开发者工具可直接导入）、`.gitignore`、`mist.d.ts` + `tsconfig.json` + `package.json`（含 `miniprogram-api-typings`，编辑器类型提示）。
- **目录构建** → 需要 `<dir>/app.mist` + `<dir>/pages/*.mist`；按微信目录规范输出（`pages/<n>/<n>.*`、`components/<k>/<k>.*`、`stores/*.js`、含页面列表的 `app.json`）。
- **单文件构建** → 平铺输出；`--app` 附带一个最小可打开的应用壳。
- **`--watch`** → 保存 `.mist`/`.ts` 即重编译（带防抖，排除输出目录）。
- 警告（`M1002` 未知 class、`M1006` 不支持的选择器、`M1008` 缺少 key 的列表、`M1012` 配置未声明对应钩子）输出到 stderr；错误带 `M` 编码、`.mist` 行列号和修复提示。
- `mistc --help` / `mistc --version` 如你所料。

## 项目结构

```
src/
├── app.mist              # 应用生命周期（onLaunch）+ 全局配置 + 全局 <style>
├── pages/
│   └── index.mist        # 页面——index 是启动页
├── components/
│   └── TodoItem.mist     # 帕斯卡命名文件 → 短横线命名组件
└── stores/
    └── stats.ts          # 跨页面共享的响应式状态（纯 TS）
```

## 语言速览

**文件**分三段：`---` TypeScript frontmatter `---`、模板、可选的 `<style>`（→ WXSS）。

**响应式**——一切静态可分析，这正是关键：

| 你写的 | 编译器生成的 |
|---|---|
| `const n = state(0)` | `data` 中的一个键 |
| `n.value++` | `this.__set('n', this.data.n + 1)` |
| `todos.value[i].done = x` | `` this.__set(`todos[${i}].done`, x) `` |
| `todos.value.push(t)` | 按长度索引的路径写入 |
| `todos.value.splice(...)` | **编译错误 M1004**（`help: 请重新赋值`） |
| `const v = derived(() => …)` | 每批次仅重算一次；带 key 的列表按*字段*级 diff |
| 模板从未读取的状态 | 完全不进入 `data`——零桥接成本（死数据消除） |
| `<input value:bind={text} />` | 原生 `model:value` + 生成的同步 handler |
| 模板中的 `{fmt(total.value)}` | 提升为生成的 derived；循环内按条目提升 |

同一事件周期内的所有写入合并为**一次** `setData`。带 `key={...}` 的 derived 数组做带键浅 diff——原位修改一项只发送 `visible[3]`，而不是整个数组。

**模板**——Web 常用标签映射为原生标签（`div`→`view`、`span`→`text`、`img`→`image`、`a href`→`navigator url`；原生标签直接透传）。`{expr}` 绑定（自动剥离 `.value`），`.map()` → `wx:for` + 必填 `wx:key`，`&&` → `wx:if`。事件：`onTap={fn}`、`onTap:catch={fn}`，带参数的内联箭头函数编译为 handler + `data-*`。Tailwind class 随处可用，包括条件表达式。

**组件**——`props({ todo: null })` → `properties`；`onXxx` prop 成为事件（`triggerEvent`），父侧自动解包参数；支持 `<slot/>` / `<slot name>`。**纯渲染组件在编译期内联**为 WXML `<template>` 片段——零组件实例开销。

**生命周期**——从 `'mist'` 导入即可：`onLoad`、`onShow`、`onPullDownRefresh`（下拉刷新）、`onReachBottom`（触底加载）、`onPageScroll`、`onTabItemTap`、`onShareAppMessage`（分享到聊天，返回分享配置）、`onShareTimeline`（分享到朋友圈）等；组件另有 `onPageShow`/`onPageHide` → `pageLifetimes`。钩子放错位置是编译错误（M1013）。

**Store**——普通 TS 模块，导出 `store(init)` 盒子和变更函数。每个订阅页面在 `data` 中持有镜像，变更时收到**路径精确、批量合并**的更新；生命周期胶水代码（`onLoad`/`onUnload`、`attached`/`detached`）自动生成。可选持久化：`store(init, { persist: 'key', version: 1, migrate })` 创建时从 `wx.getStorageSync` 恢复、变更后防抖写回。

**Tailwind v4**——由真实的 `@tailwindcss/cli` 按用量生成工具类；Rust 后处理器把现代 CSS 重写为 WXSS（展开 `@layer`、`:root`→`page`、`oklch()`→十六进制、`color-mix`→`rgba`、`rem`→`rpx`（1rem = 32rpx）、`rounded-full` 的 `calc(infinity*1px)`→`9999px`、WXSS 无法表达的选择器**带警告地**移除）。class 名在标记和 CSS 中做完全一致的净化（`w-[32px]` → `w-_32px_`）。所有单元共享 `tw-shared.wxss`——顺带解决了自定义组件样式隔离——`page {}` 主题变量拆分到仅页面引入的 `tw-theme.wxss`。

## 基准测试

与 **Taro 3.6.35 + React 18** 在真实微信开发者工具（基础库 3.17.0）中一对一对比，使用同一套与框架无关的测量工具（[benchmark/devtools/](benchmark/devtools/)）：`setData` 在页面对象上挂钩（位于任一框架运行时之外）、相同的脚本化点击、同一台机器。

**列表应用**——1000 行过滤列表，50 次行切换：

| 指标 | Mist | Taro 3 + React |
|---|---|---|
| 点击延迟 p50 / p95 | **68 / 113 ms** | 162 / 180 ms |
| 每次点击 setData 次数 | 1 | 1 |
| 每次点击字节数 | **26 B** | 67 B |
| 初始数据体积 | **49.5 KB** | 140 KB |
| 切换过滤器 | **72 ms / 32 KB** | 78 ms / 80 KB |
| 包体积 | **9.6 KB** | 293 KB |

**商城应用**——100 件商品、带数量的购物车、组件事件、3 个 derived、50 次加购：

| 指标 | Mist | Taro 3 + React |
|---|---|---|
| 点击延迟 p50 / p95 | 67 / 81 ms | 57 / 89 ms |
| 每次点击字节数 | **84 B** | 286 B |
| 初始数据体积 | **5.2 KB** | 22.2 KB |
| 切换过滤器数据量 | **1.3 KB** | 14.2 KB |
| 包体积 | **11.6 KB** | 294.9 KB |

**方法与局限——引用这些数字前请先阅读：**

- 在**微信开发者工具中测量，不是真机**。手机 WebView 通常会放大包解析和桥接成本，但在完成真机测量之前，这些只是模拟器级别的数字。
- 只对比了一个 Taro 版本（3.6.35 + React 18，webpack5 生产构建）——不是 Taro 4，也不是其他框架。
- 延迟由自动化工具驱动（含 websocket 往返）；只在同一测量框架内可比，不能与手动点击比较。
- 商城应用显示**小列表下点击延迟趋同**——100 行的 React 协调很便宜，测量开销占主导。Mist 的延迟优势是大列表现象；数据量优势随数据复杂度增长（结构性变更时 11 倍）。
- 复现方法：`benchmark/devtools/README.md`——Taro 对照应用已提交并锁定版本。

## 工作原理

约 5k 行 Rust（[完整架构图见 AGENTS.md](AGENTS.md)）：

1. **`sfc`** 切分文件（记录行偏移供诊断使用）
2. **`frontmatter`** 用 [oxc](https://oxc.rs) 解析 TS，做*基于 span 的源码改写*——不做代码生成；变更经 AST 访问器变为路径写入，读取经受控正则改写
3. **`template`** 解析类 JSX 标记；**`wxml`** 生成 WXML 与 handler 契约
4. **`tailwind_cli`** 运行真实 Tailwind 并把现代 CSS 重写为 WXSS
5. **`lib`** 编排项目图（组件、内联决策、store、目录布局），**`main`** 写出微信目录树

生成的 JS 刻意保持可读（微信开发者工具无法加载外部 source map）：普通的 `Page({...})` 对象、保留你的命名，外加 `require('mist-rt.js')`——约 9 KB 的运行时，负责批量合并、带键 diff、store 订阅，以及 setData 被拒绝时的状态回滚。

## 当前状态

端到端可用并在微信开发者工具中验证：页面、组件、slot、内联、store、Tailwind v4、项目构建、诊断、基准测试——350+ 个测试（以 `cargo test` 为准）。它仍是**原型**，但语言核心已完整：路径精确 setData 的响应式、带键字段级 diff 的 derived、死数据消除、组件/slot/内联、store、`value:bind` 输入、模板表达式提升（含按条目）、Tailwind v4、经 `app.mist` 配置的 tabBar、查询参数路由、完整交互生命周期（下拉刷新、触底、分享/朋友圈钩子、组件 pageLifetimes）、可选 store 持久化、`<style scoped>`、Node 测试环境（`mistc test`，支持 `setData` payload 大小断言）、原生标签属性/事件校验（M1023/M1024）、`[id].mist` 路由参数页面、编辑器类型（`mist.d.ts` + wx 类型），以及 `M1001` 别名变更分析。仍处于设计阶段：npm 导入。精确的「已实现 vs 规范」对照表见 [AGENTS.md](AGENTS.md)。

## 路线图

1. 真机基准数据（需要已注册的 AppID）
2. 嵌套循环提升
3. 零 Node 方案：打包 Tailwind 独立二进制
4. `mistc-lsp`——诊断、补全、悬停、跳转定义、签名帮助、增量同步、重命名（含跨文件 store 重命名）以及 [editors/vscode](editors/vscode) 客户端均已可用；下一步：工作区级诊断

## 许可证

MIT——见 [LICENSE](LICENSE)。
