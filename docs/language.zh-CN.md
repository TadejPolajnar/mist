# Mist 语言指南

[English → language.md](language.md)

本页所有内容均已实现并通过测试。若 SPEC.md 承诺的功能超出本页，SPEC 只是路线图；本页才是产品本身。

## 文件结构

一个 `.mist` 文件最多包含三段：

```
---
// TypeScript frontmatter — parsed by a real TS parser (oxc)
---
<!-- template — JSX-ish markup -->
<style>/* optional, compiled to WXSS */</style>
```

frontmatter 是必需的。文件分为页面（`pages/*.mist`）、组件（PascalCase 文件名，由页面导入）或 `app.mist`（仅应用壳：生命周期 + `config` + 全局 `<style>`；在其中写状态和模板是编译错误）。

## 响应式

```ts
import { state, derived } from 'mist'

const count = state(0)
const todos = state([{ id: 1, title: 'hi', done: false }])
const open  = derived(() => todos.value.filter(t => !t.done))
```

读取用 `x.value`。写入是**编译出来的**，不是运行时观察的——每一次变更都编译为它所改变的精确 `setData` 路径：

| 你写的 | 经桥发送的内容 |
|---|---|
| `count.value++` | `{ count: <new> }` |
| `todos.value[i].done = true` | `` { `todos[${i}].done`: true } `` |
| `todos.value.push(t)` | `` { `todos[${len}]`: t } `` |
| `todos.value = [...]` | 一次整键写入 |
| `todos.value.splice(...)` | **编译错误 M1004**——请改用重新赋值 |

一个事件 handler 内的所有写入按页面批量合并为**一次** `setData`。

**派生值**每批次仅重算一次。带 `key` 渲染的派生数组获得键控、字段级 diff：切换一项只发送 `{ 'open[3].done': true }`，别无其他。长度/顺序变化回退为一次整键写入。派生值是只读的。

**死数据消除：**模板从未读取的状态（例如只渲染 `open` 时的 `todos`）完全不进入 `data`——它作为实例字段存在，零桥接成本，变更只触发派生值重算。这是自动的。

**普通常量：**顶层的、不是 `state()`/`derived()` 的 `const` 可以在模板中直接引用（不带 `.value`）——`{TABS.map(t => …)}` 适用于静态查找表。被模板引用的常量作为静态值一次性写入 `data`：没有响应式，没有 diff，之后的变更不产生任何效果。模板从未引用的常量保持为普通 JS，永远不进入 `data`。

**编译器强制执行的规则：**只能通过 `x.value...` 路径变更。使用响应式名称而不带 `.value` 是 M1007 错误。取别名到局部变量再通过它写入（`const t = todos.value[0]; t.done = true`）——或通过 `for...of` 变量、`forEach` 回调参数变更——是 M1001 错误，附带修复提示。除 `push`/索引赋值以外的数组方法是 M1004 错误。缺少 `key=` 的 `.map()` 产生警告（M1008）：它会回退为整数组写入。

## 模板

Web 常用标签映射为原生标签；原生标签原样透传：

| 你写的 | 生成的 |
|---|---|
| `div section header footer main article ul ol li p h1–h6 nav aside` | `view` |
| `span` | `text`（裸文本/`{expr}` 直接输出，不包裹）——其中出现盒式样式的子元素（`div` 等）会警告 M1018，因为原生 `text` 忽略盒式样式 |
| `img` | `image` |
| `a href="/pages/x/x"` | `navigator url="…"` |
| `button input scroll-view swiper …` | 透传 |

**绑定**——`{expr}` → `{{expr}}`，并剥离 `.value`。成员访问、算术、比较、三元、`&&`/`||`、字符串拼接都在 WXML 中内联执行。**WXML 无法求值的内容会自动提升**——函数调用（包括 `Math.*` 和带非响应式参数的调用）、模板字符串和可选链编译为生成的派生值，每批次重算；循环内的 `{fmtDate(t.ts)}` 成为每个条目上的计算字段（`t._c0`），并保留键控 diff。限制（M1009 错误，绝不静默）：*嵌套*循环内的调用，以及引用循环条目的模板字符串/可选链——请把它们预先计算为计算字段或派生值。

**列表**——`.map()` 加**必填的 key**：

```jsx
{todos.value.map(t => (
  <div key={t.id} onTap={() => toggle(t.id)}>{t.title}</div>
))}
```

`key` 必须是直接属性（`t.id`）、条目本身（`key={t}` → 基本类型的 `*this`）或 `index`（允许，但会禁用键控 diff）。计算键或深层键是 **M1003** 错误。

第二个回调参数绑定循环索引并生成 `wx:for-index`：

```jsx
{todos.value.map((t, i) => (
  <div key={t.id}>{i}: {t.title}</div>
))}
```

`key={i}`（索引参数自身的名字）也按 `key={index}` 处理。只支持 `(item)` 和 `(item, index)` 两种形式——解构参数和额外参数是 **M1010** 错误。

**条件**——`{cond && <jsx/>}` → `wx:if`。非 JSX 的 `&&` 保持为绑定。

`{cond ? <a/> : <b/>}` → `wx:if` / `wx:else`。两个分支都必须是 JSX 元素（把文本分支包进 `<span>`，或对单分支使用 `&&`）。`else` 分支里的三元——`{a ? <x/> : b ? <y/> : <z/>}`——链式生成 `wx:elif`。非 JSX 三元（`{cond ? 'a' : 'b'}`）保持为绑定。

**事件**——`on` + 首字母大写的事件名：

```jsx
<button onTap={save}>save</button>            // bindtap="save"
<div onTap:catch={f}>…</div>                  // catchtap (stops propagation)
<div onTap:mut={f}>…</div>                    // mut-bind:tap
<input onInput={setQuery} />                   // handler receives the native event
<div onTap={() => del(item.id, 'soft')}>…</div>
```

`onClick` 是 `onTap` 的别名。裸标识符直接绑定（方法接收原生微信事件）。内联箭头函数只能是 `() => method(args…)` 形式——参数通过 `data-*` 属性捕获，因此必须是可序列化的表达式。带事件参数的箭头函数不受支持；请使用裸方法并读取 `e.detail`/`e.currentTarget.dataset`。

当你同时需要按条目参数和原生事件时，绑定裸方法并自己携带参数：

```jsx
{todos.value.map(t => (
  <input key={t.id} data-id={t.id} onInput={rename} />
))}
```

```ts
function rename(e) {
  const id = e.currentTarget.dataset.id
  const text = e.detail.value
}
```

（编译器生成的内联箭头参数使用同一机制，名字为生成的 `data-a0`、`data-a1`……）子组件的回调 prop 参数在父侧以同样方式到达——生成的包装器自动把 `e.detail.args` 解包回你的 handler 参数。

**Class**——Tailwind 随处可用，包括条件表达式：

```jsx
<span class={t.done ? 'line-through text-gray-400' : ''}>{t.title}</span>
```

带特殊字符的 class 名在标记和 CSS 中做完全一致的净化（`w-[32px]` → `w-_32px_`），因此任意值可用。

## 组件

```jsx
---
// components/TodoItem.mist
import { props } from 'mist'
const { todo, onToggle } = props({ todo: null })
---
<div onTap={() => onToggle(todo.id)}>
  <span>{todo.title}</span>
</div>
```

- `props({...})` 带默认值解构 → 微信 `properties`。值必须可序列化。
- `props<T>()` 把类型参数映射到微信的 `type:` 字段：`string` →
  String、`number` → Number、`boolean` → Boolean、`T[]`/元组 → Array、对象
  类型字面量和类型引用（接口、`Record<...>`）→ Object。
  字面量联合 prop（例如 `'sm' | 'lg'`）映射为它们共同的基本类型。
  混合联合、`any`/`unknown` 和其他无法解析的类型回退为
  `type: null`（微信自身的默认值——不做转换，不报不匹配警告）。
  指向基本类型的类型别名（例如 `type Id = number`）仍映射为 Object，
  因为别名无法解析回其底层类型。
- 名为 `onXxx` 的 prop 是**回调 prop**：子组件像函数一样调用它们；它们
  编译为 `triggerEvent('xxx', { args })`，父组件的
  `<TodoItem onToggle={toggle} />` 自动生成解包参数的 `bind:toggle`
  包装器。回调参数必须可序列化。
- `<slot/>` 和 `<slot name="x"/>` 可用；具名 slot 自动启用
  `multipleSlots`。WXML 中不存在作用域 slot 和 slot 回退内容。
- 组件像页面一样使用自己的状态/派生值/生命周期
  （`onCreate` → `created`、`onAttach` → `attached`、`onReady` → `ready`、
  `onMove` → `moved`、`onDetach` → `detached`）。`onCreate` 在
  `properties`/`data` 存在之前运行——在其中写状态是编译错误（M1017）；
  只用它来初始化非响应式实例字段。页面另有自己的
  `onRouteDone`（在路由进入动画之后触发）和
  `onSaveExitState`（返回 `{ data, expireTimeStamp? }`，把恢复快照交给
  微信）；两者都仅限页面。

**自动内联：**严格纯渲染的组件——只有数据 prop；没有状态、派生值、函数、回调 prop、事件、slot、生命周期、导入或 `config`——永远不会成为微信组件。它编译为内联进各父级的 WXML `<template>` 片段：没有实例开销，没有 JS，样式合并。**注意样式后果**：内联组件的普通 `<style>` 合并进父页面（页面级作用域），而真正的组件获得微信的按组件样式隔离——用 `<style scoped>`（见「样式」一节）让内联组件的 class 只作用于自身，或者关闭内联。除非查看 `dist/`，否则这在其他方面不可见。用 `export const config = { inline: false }` 关闭内联——组件随即编译为真正的 `Component()`；`inline` 键仅存在于编译期，永远不会进入生成的 `.json`。

非内联组件默认生成 `"styleIsolation": "isolated"`；在 `config` 中设置 `styleIsolation` 可覆盖，例如 `'apply-shared'` 允许页面样式级联进来。

**组件选项——`virtualHost`、`pureDataPattern`、`externalClasses`：**仅组件可用的 `config` 键，仅存在于编译期（永远不会进入生成的 `.json`）：

```jsx
export const config = {
  virtualHost: true,
  pureDataPattern: '^_',
  externalClasses: ['x-class'],
}
```

- `virtualHost: true` 移除组件自身的包裹节点——子组件的根元素直接渲染进
  父级，没有额外层级。适用于有真实渲染成本的列表项式组件。设置它时
  mistc 不会替你更改 `styleIsolation`；在组合使用前，请查阅微信文档中
  `virtualHost` 与 `styleIsolation` 的交互方式。
- `pureDataPattern: '<pattern>'` 把匹配该正则的数据字段标记为非渲染
  字段——微信在 WXML 重渲染时跳过它们。字符串不得包含 `/` 或 `\`
  （它编译为 JS 正则字面量，例如 `/^_/`，末尾反斜杠会转义结束定界符）；
  包含时编译失败——请使用 `'^_'` 这样的简单前缀。
- `externalClasses: ['x-class', ...]` 声明父级可以填充的 class 槽位：
  父级模板中的 `<my-comp x-class="red-text" />` 是普通 WXML
  属性透传——没有特殊的 mist 语法。每一项只能包含
  字母/数字/`-`/`_`。
- 三者都仅限组件；在页面或 `app.mist` 中使用是编译错误。

**冒泡的回调事件：**默认情况下回调事件只到达直接父级。在 `config` 中设置 `events`，为回调 prop 添加 `triggerEvent` 选项，让孙组件无需每个中间组件手动转发回调即可通知祖父组件：

```jsx
export const config = { events: { onToggle: { bubbles: true, composed: true } } }
```

这编译为 `this.triggerEvent('toggle', { args }, { bubbles: true, composed: true })`。
微信的 `bubbles` 让事件沿祖先节点上冒；要跨越组件边界还必须加上
`composed`——没有它，仅 `bubbles: true` 会停在所在组件内。`events`
的键必须指向已声明的回调 prop；`events` 键仅存在于编译期，永远不会进入
生成的 `.json`。

## Store——跨页面共享状态

```ts
// stores/cart.ts — plain TypeScript
import { store } from 'mist'

export const cart = store({ items: [], total: 0 })

export function add(item) {
  cart.value.items.push(item)
  cart.value.total += item.price
}
```

```jsx
---
import { cart, add } from '../stores/cart.ts'
---
<span>{cart.value.total}</span>
```

任何导入 store 的页面/组件获得一个实时镜像：读取像本地状态一样绑定，导入的函数随处可用（包括作为事件 handler），而每一次变更——来自任何页面——都以**路径精确、批量合并的 `setData`** 到达所有订阅页面。订阅生命周期（`onLoad`/`onUnload`、`attached`/`detached`）自动生成。

Store 模块规则：只能从 `'mist'` 导入；只能导出 `store()` 值和函数；同样的变更编译规则也适用于 store 函数内部。

**持久化**——按 store 选择启用：

```ts
export const cart = store({ lines: [] }, { persist: 'app.cart', version: 2, migrate })

function migrate(old, oldVersion) {
  return { lines: old.lines || [] }
}
```

`persist` 指定 `wx` 存储键。store 在模块加载时从存储恢复（hydrate）；变更防抖写回（约 200 ms），并在 `wx.onAppHide` 时做最终落盘。当已保存数据信封的 `version` 不同时，`migrate(old, oldVersion)` 把旧数据映射为当前结构——其返回值立即存回，返回 `undefined` 则回退为 `init`。没有 `migrate` 时，版本不匹配会丢弃已保存的数据。

## 插件——微信原生组件/API

微信插件（地图、支付供应商、直播、客服）是不透明的运行时外部对象——永远不是响应式状态，永远不会被打包。

在 `app.mist` 的 `config` 中声明插件（原样透传）：

```ts
export const config = {
  plugins: {
    calendarPlugin: { version: '1.0.0', provider: 'wx1234567890abcdef' },
  },
}
```

用来自 `plugin://<name>` 的默认导入引入插件的 JS 接口：

```ts
import calendar from 'plugin://calendarPlugin'

function open() {
  calendar.select()
}
```

编译为：

```js
const calendar = requirePlugin('calendarPlugin');
```

只支持对整个插件的默认导入——具名导入（`import { x } from 'plugin://...'`）和空/非法名称是 **M1015** 错误。

用 `config.pluginComponents` 注册供模板使用的插件**组件**——编译期提取，合并进生成的 `usingComponents`，其本身永远不会进入 `.json`：

```ts
export const config = {
  pluginComponents: { calendar: 'plugin://calendarPlugin/calendar' },
}
```

```jsx
<calendar />
```

`pluginComponents` 的名称与导入的 `.mist` 组件标签冲突是 M1015 错误。

任何 mistc 无法归类的标签——不是原生微信组件、不是 Web 别名、也未通过 `.mist` 导入、`pluginComponents` 或手动 `usingComponents` 注册——会得到 **M1019** 警告（微信把未知标签渲染为空）。对你确定在别处已处理的标签，用 `config.customTags` 静默该警告：

```ts
export const config = {
  customTags: ['my-web-component'],
}
```

`customTags` 的条目在编译期消费（只允许字母/数字/`-`/`_`），永远不会进入 `.json`。

日常原生标签上的事件和属性也以同样的方式检查：打错字的
`onScrolToLower` 或 `scrol-y`——微信会静默忽略它们——会得到
**M1023**/**M1024** 警告并附带纠正建议。只有编译器元数据表中的标签会被
检查，`config.customAttrs`（与 `customTags` 同形）可放行表里不认识的名字。

## 样式

- **Tailwind v4**——真实的 `@tailwindcss/cli` 按你的 class 用量运行；输出
  为 WXSS 重写（`rem`→`rpx`，1rem = 32rpx；`oklch()`→十六进制；媒体查询
  保留；`page{}` 主题变量拆分到仅页面引入的样式表）。
  WXSS 无法表达的选择器（`hover:`、`space-x-*`……）被移除并附带
  **M1006** 警告——绝不静默。
- `bg-gradient-to-*`/`from-*`/`via-*`/`to-*` 渐变在 sRGB 中插值——
  颜色插值提示（`in oklab` 等）被剥离，以兼容较旧微信 webview 的
  设备。
- `<style>` 块原样编译为该单元的 `.wxss`。
- **`<style scoped>`** 把样式块的作用域限定在本单元：每个 class 选择器获得
  可读的按单元后缀（`.card` → `.card--todo-item`），并在 WXSS 与该单元的
  标记（`class`、`hover-class`、`placeholder-class`，包括三元表达式、`class:list`、模板字符串
  和被提升的 class 表达式里的字符串字面量）中做完全一致的改写。这正是让
  **内联**组件可以安全携带样式的机制——合并后的样式不会再与父级冲突。
  `@media`/`@supports` 的内容递归处理；`@keyframes` 和标签选择器保持不变
  （keyframe 名仍是全局的）。一个限制：**由 frontmatter 函数返回**的
  class 名（如 `class={cls()}`，字符串在 `cls` 内部拼出）不会被改写——
  请把需要作用域的 class 字面量写在模板里。`app.mist` 的样式不能加
  scoped——它天然就是全局的。
- `app.mist` 的 `<style>` 成为 `app.wxss`（全局；`page { … }` 在那里有效）。
- **设计令牌**——把 `src/theme.css` 放在 `app.mist` 旁边，为整个项目
  定义 Tailwind v4 令牌和自定义工具类：

  ```css
  @theme {
    --color-primary: #07c160;
    --text-cell: 17px;
  }
  @utility pb-safe {
    padding-bottom: env(safe-area-inset-bottom);
  }
  ```

  模板随后像使用任何工具类一样使用 `bg-primary`、`text-cell`、`pb-safe`。
  该文件拼接进 Tailwind 构建输入；令牌定义以
  `page { --… }` 变量的形式输出，因此它们也级联进组件。不要把它
  与 `theme.json`（下文的微信深色模式文件）混淆。主题编辑
  会自动使记忆化的 CSS 缓存失效。
- `class:list={[...]}` 组合 class：字符串字面量、`cond && 'classes'`
  和 `{ class: cond }` 对象——所有字面量都参与 Tailwind 生成，
  条件编译为 WXML 三元。请在元素上用它*替代* `class`，
  不要与 `class` 同时使用。

```jsx
<div class:list={['p-4', open.value && 'font-bold', { hidden: done.value }]} />
```

## 配置与导航

- `export const config = {...}`（仅限静态字面量）→ 页面/应用的 `.json`。
  在 `app.mist` 中它与生成的页面列表合并。列表把
  `index` 排在最前（成为启动页），其余按字母顺序——
  在 `app.mist` 的 `config` 中设置 `entryPagePath` 可启动其他页面。
- `app.mist` 接受 `onLaunch`、`onShow`、`onHide`、`onError`、
  `onPageNotFound`、`onUnhandledRejection`、`onThemeChange`——任何其他钩子
  都会被拒绝（M1013）。最后四个（`onError` 到 `onThemeChange`）为应用
  专属；在页面或组件中声明它们同样被拒绝（M1013）。
- 导航方式：`<a href="/pages/about/about">`（→ `navigator`）、直接使用
  `wx.*` API（`wx` 完全可用；Mist 不包装任何东西），或下文的类型化
  `navigate()` 内建函数。
- 把 `sitemap.json` 放在 `app.mist` 旁边可控制微信搜索
  索引；没有它时，mistc 生成一个空规则集。
- 深色模式：在 `app.mist` 的 `config` 中设置 `darkmode: true` 和
  `themeLocation: "theme.json"`，并把 `src/theme.json` 放在 `app.mist` 旁边。mistc
  把它原样复制到 `dist/theme.json`。没有源文件时，`dist` 中不会出现
  `theme.json`，构建也不会失败或警告。

### `navigate()`——类型化路由

`import { navigate } from 'mist'` 把路由调用编译为匹配的 `wx.*` 导航 API，并且——对目录构建（`mistc build <dir>`）——在编译期对照已编译页面列表检查路由（M1021）：

```ts
import { navigate } from 'mist'

navigate('/pages/detail/detail', { id: 3 })   // → wx.navigateTo({ url: '/pages/detail/detail' + <query> })
navigate.replace('/pages/detail/detail')       // → wx.redirectTo({ url: '/pages/detail/detail' })
navigate.back()                                // → wx.navigateBack()
navigate.back(2)                               // → wx.navigateBack({ delta: 2 })
navigate.switchTab('/pages/home/home')         // → wx.switchTab({ url: '/pages/home/home' })
```

路由参数必须是字符串字面量（不含 `${}` 插值的普通模板字符串也算）——编译器需要看到确切的字符串才能检查。标识符、带插值的模板字符串或字符串拼接都会以 M1021 失败，不在已编译页面列表中的字面量路由同样如此。`navigate.switchTab` 还要求路由是 `app.mist` 的 `tabBar.list[].pagePath` 条目之一——当该列表是带字符串 `pagePath` 的对象字面量静态数组时。

可选的 `params` 对象（`navigate()` 和 `navigate.replace()` 接受）在运行时序列化为 `?key=value&...` 查询字符串并追加到路由；值经过 `encodeURIComponent` 处理。

路由检查只适用于目录构建——`mistc build <dir>` 知道完整页面列表（`src/pages/` + `src/packages/*/pages/`）；平铺/单文件构建（`mistc build <file>`）没有可对照的页面列表，其中的 `navigate()` 调用编译时不做路由校验。

`mistc build`（仅目录构建）还会在项目根目录已存在的 `mist.d.ts`（`mistc init` 脚手架生成的文件）旁写出 `mist-routes.d.ts` 文件——它把 `navigate()` 的 `route` 参数从 `string` 收窄为所有已编译页面路由的联合类型，于是未知路由在你的编辑器里也是类型检查错误，而不仅是编译期错误。它在每次构建时重新生成，没有可依附的 `mist.d.ts` 时静默跳过。它永远不会写入 `dist/`，也永远不会被构建清单跟踪。

## 分包

把页面放在 `src/packages/<pkg>/pages/*.mist` 下即可构建微信分包。每个 `<pkg>` 名称只能使用字母、数字、`-` 或 `_`，且不能是 `pages`、`components`、`stores` 或 `assets`（这些是保留的 dist 路径）。mistc 发现每个 `src/packages/<pkg>/pages/*.mist` 文件，并把它编译为 `packages/<pkg>/pages/<name>/<name>.*`。

`app.mist` 生成的 `app.json` 按来源拆分页面：

```json
{
  "pages": ["pages/index/index"],
  "subPackages": [
    { "root": "packages/shop", "name": "shop", "pages": ["pages/cart/cart"] }
  ]
}
```

- `pages` 只列出主包页面（`src/pages/*.mist`）。
- `subPackages` 按 `<pkg>` 分组分包页面；每一项的 `pages` 列表
  相对于 root（`pages/cart/cart`，而不是 `packages/shop/pages/cart/cart`）。
- 主包在 `src/pages/` 中至少需要一个页面——只有分包页面的项目
  构建失败。
- `subPackages` 由编译器生成：在 `app.mist` 的 `config` 中设置它
  会被拒绝（M1014）。

**`preloadRule`**：mistc 不生成——请自行在 `app.mist` 的
`config` 中设置，它会直接透传到 `app.json`：

```ts
export const config = {
  preloadRule: {
    "pages/index/index": { network: "all", packages: ["shop"] },
  },
}
```

**动态加载**：在 `app.mist` 的 `config` 中设置
`lazyCodeLoading: "requiredComponents"`（透传，非生成）——微信建议
大多数项目启用。页面 `config` 中的 `componentPlaceholder` 同样
原样透传。

**当前不支持**：异步加载的分包*组件*（声明在
`src/packages/<pkg>/` 内、由微信随分包懒加载的
组件）。被分包页面导入的组件只编译一次，位于
它通常的主包路径（`components/<k>/<k>.*`）——分包页面
用深度为 4 的相对路径引用它，与引用
`mist-rt.js` 的方式相同。目前没有办法声明一个随分包一起发布
且只在该分包加载时才加载的组件。独立
分包同样不支持：它们无法 `require` 主包
运行时（`mist-rt.js`），而每个编译单元都依赖它。

## 静态资源

在项目目录的每次构建中（平铺单文件构建除外），`src/assets/**` 被原样复制到 `dist/assets/**`。在模板或 `app.mist` 的 config 中以 `/assets/...` 或相对路径引用这些文件，例如 `tabBar.list[].iconPath: "assets/tab-home.png"`。隐藏文件（以 `.` 开头的名称）被跳过。已删除的源文件在重建时从 `dist` 中清理。`assets/` 内的符号链接被跳过。

## Workers

在 `app.mist` 的 `config` 中设置 `workers: "workers"` 以启用微信 worker 线程，并把普通 JS 文件放在 `src/workers/**` 下。在项目目录的每次构建中（平铺单文件构建除外），mistc 把该目录原样镜像到 `dist/workers/**`——不编译，不校验 JS。已删除的源文件（以及清空的子目录）在重建时从 `dist` 中清理。

## 自定义 tab bar

把 `src/custom-tab-bar.mist` 放进项目即可构建品牌化 tab bar。mistc 把它编译到微信要求的固定 dist 路径：`custom-tab-bar/index.{js,json,wxml,wxss}`。它像任何其他组件一样编译——状态、共享 `.mist` 组件的导入和 store 导入都可用。

`app.mist` 的 config 必须在常规的 `tabBar.list` 之外设置 `tabBar.custom: true`：

```ts
export const config = {
  tabBar: {
    custom: true,
    list: [
      { pagePath: "pages/index/index", text: "Home", iconPath: "assets/tab-home.png" },
      { pagePath: "pages/cart/cart", text: "Cart", iconPath: "assets/tab-cart.png" },
    ],
  },
}
```

每个显示 tab bar 的页面在自己的逻辑中调用 `getTabBar()`（原生微信 API，不是 Mist 包装）来同步激活的 tab，通常在 `onShow` 中：

```ts
import { onShow } from 'mist'

onShow(() => {
  const tabBar = getTabBar()
  if (tabBar) {
    tabBar.setData({ active: 0 })
  }
})
```

`wx.*` 的 tab bar API（`wx.setTabBarItem`、`wx.showTabBar`……）保持不包装、随时可用，与 Mist 中其他任何地方相同。

文件和配置标志必须一致，否则构建会警告/报错（M1020）：有 `tabBar.custom: true` 而没有该文件是错误（微信会渲染空白 tab bar）；有该文件而没有该标志是警告（微信忽略该文件并渲染内置 tab bar）。

## 双向绑定

`<prop>:bind={state}` 把一个状态双向绑定到原生元素属性。变化通过原生 `model:<prop>` 渲染（没有 setData 回声），同时生成的 `__vb_<state>` handler 保持逻辑侧镜像和派生值同步。手动事件 handler（`onInput`、`onChange`）仍可代替使用。

| `<prop>:bind` | model 属性 | 配套事件 |
|---|---|---|
| `value:bind` | `model:value` | `bindinput` |
| `checked:bind` | `model:checked` | `bindchange` |

示例：`<input value:bind={text} />`、`<switch checked:bind={on} />`。

只支持这两个属性。任何其他 `<ident>:bind`（例如 `foo:bind`）都是编译错误。`.mist` 子组件上的双向绑定（自定义组件上的 `model:`）尚不支持。

## 路由参数页面——`pages/item/[id].mist`

详情页可以在文件名里声明它的查询参数：`pages/item/[id].mist` 编译为普通
路由 `pages/item/item`（微信没有动态路径——这只是**查询参数之上的语法
糖**，仅此而已）。frontmatter 必须声明 `const id = state(...)`；编译器随后
生成每个详情页原本要手写的东西：

- **缺参守卫**——`onLoad` 没有收到 `id` 时打印错误并 `wx.navigateBack()`，
  而不是渲染一个坏页面；
- **注入**——在你的 `onLoad`（通常不再需要）运行之前，`id.value` 已从
  查询参数设置好；
- **类型化路由条目**——`mist-routes.d.ts` 获得 `RouteParams` 条目，
  `navigate('/pages/item/item', { id })` 必须传参，不传是类型错误。

查询参数到达时是字符串——需要数字时在 `derived` 里转换。每个目录只允许
一个 `[param].mist`；`pages/item.mist` 与 `pages/item/[id].mist` 并存是
编译错误（它们会冲突）。分包同样可用
（`packages/<pkg>/pages/<dir>/[id].mist`）。见 examples/portfolio 的持仓
详情页。

## npm 导入——`import dayjs from 'dayjs'`

页面和组件中可以使用裸 npm 导入（store 模块暂不支持）。在项目根目录
`npm install` 依赖；编译器用 esbuild（像 Tailwind 一样一次性安装到
`~/.cache/mistc/`）把每个导入的包打包为自包含的
`dist/vendor/<pkg>.js`，并生成普通的 `require`。支持默认、具名和子路径
导入；不支持 `* as` 命名空间导入。

这里有一个刻意的限制——**npm 代码是不透明边界**。编译器的整个论点是
追踪响应式状态的每一次变更；它无法看进打包后的库。因此把响应式值
（state、derived 或 store 镜像）作为参数传给导入的函数是编译错误
（**M1026**）。先把函数需要的内容拷贝到普通局部变量：

```ts
import { format } from 'date-fns'

const when = state({ ts: 0 })
const label = state('')

function f() {
  format(when.value, 'yyyy')       // ✗ M1026 —— 响应式对象越过了边界
  const ts = when.value.ts
  label.value = format(ts, 'yyyy') // ✓ 原始值拷贝进去，普通返回值出来
}
```

返回值是普通数据——赋给 state 没有问题。检查覆盖直接调用（包括
`dayjs.utc(...)` 这样的成员调用）；通过别名或回调转交响应式值属于
M1001 记录的同一类不可追踪边界。打包只在项目构建中进行——单文件构建
会生成 `require` 但不产出 vendor 文件。编译器暂不检测依赖 DOM 或 Node
API 的包——它们能打包成功但会在微信 JS 环境中运行时报错；请只使用纯
计算类库。

## 尚未支持（路线图——这些功能今天会给出明确的错误）

嵌套循环内的调用（M1009）、store 模块内的 npm 导入。Tab bar/window 配置：把 `tabBar` 放进 `app.mist` 的 `config`——不需要单独的配置文件。

TypeScript 注解（参数/返回类型、`interface`、`type`、`as`、泛型、`import type`）在生成前被剥离——放心添加注解；它们都不会进入生成的 JS。`enum` 是例外：它是运行时构造，会被拒绝并附带修复提示（请使用 const 对象或字符串字面量联合类型）。
