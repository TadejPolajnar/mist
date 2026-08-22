# 诊断说明

[English → diagnostics.md](diagnostics.md)

每个错误都带有编码和修复提示；M1001、M1004、M1007、M1011、M1013、M1017、
M1021 和 M1022 会给出文件的行:列位置，M1010 给出行号（其余编码只报告文件
路径；M1015 对导入形态错误给出行:列，对 `pluginComponents` 的取值/冲突检查
只报告文件路径）。警告（`M1002`、`M1006`、`M1008`、`M1012`、`M1016`、
`M1018`、`M1019`、`M1020`、`M1023`、`M1024`、`M1027`、`M1028`、`M1029`）输出到 stderr，永远不会导致构建失败。M1020 在
上述警告中只是名义上的例外——当 tab bar 文件存在而配置标志缺失时它是警告，
当设置了标志却缺少文件时它是错误（见下文）。

## M1001 — 别名状态变更

通过本地别名写入状态不会编译出任何响应式代码——写入只发生在内存中，不会
发出 `setData`。

```ts
const t = todos.value[0]
t.done = true                 // ✗ M1001
todos.value[0].done = true    // ✓ 路径精确的 setData
```

该检查适用于通过 `state()` 或 `store()` 值的别名进行的成员/索引写入、
`++`/`--`，以及会变更数组的调用（`push` 等）——包括 `for...of` 循环变量，
以及对状态路径调用 `forEach`/`map`/`filter`/`find`/`some`/`every`/`flatMap`
时的回调参数。只读别名没有问题。追踪是作用域感知的：同名遮蔽的参数或局部
变量只在其所属函数内关闭该检查，对别名重新赋值（`t = other`）会使其失去
别名身份。副本（`.slice()`、展开运算符）永远不是别名。编译器看不到的写入
（传入辅助函数的别名）仍由你自己负责：请始终通过完整的 `x.value…` 路径
写入。

## M1002 — 未知 class（警告）

某个模板 class 没有产生任何 CSS。通常是拼写错误，或者是一个不存在的
Tailwind 工具类。在用户 `<style>` 块中定义的 class——单元自己的块或
`app.mist` 的全局块——会从选择器文本中收集，永远不会触发警告，因此手写的
CSS class 不受影响。

## M1003 — 无效的列表 key

`wx:key` 只接受循环项的**直接属性**、循环项本身或 `index`。

```jsx
{items.value.map(t => <li key={t.a + t.b}>…</li>)}   // ✗ 计算表达式
{items.value.map(t => <li key={t.meta.id}>…</li>)}   // ✗ 深层路径
{items.value.map(t => <li key={t.id}>…</li>)}        // ✓
{tags.value.map(t => <li key={t}>…</li>)}            // ✓ 原始值 → *this
{xs.value.map(x => <li key={index}>…</li>)}          // ✓ 但会关闭键控 diff
```

修复：给你的列表项加一个稳定的 id。

## M1004 — 无法编译的数组变更

对状态调用 `pop / splice / shift / unshift / sort / reverse` 无法编译为
精确的 `setData` 路径。

```ts
items.value.splice(i, 1)                        // ✗ M1004
items.value = items.value.filter((_, j) => j !== i)   // ✓ 一次整键写入
```

`push` 和索引赋值保持路径精确，可以使用。

## M1005 — 命名冲突

状态、派生、props、方法、store 导入和 store 函数共用一个命名空间（它们
都会成为同一个对象的键）。请重命名报告的两个声明之一。

## M1006 — 被移除的选择器（警告）

Tailwind 生成了 WXSS 无法表达的选择器（`hover:`、`:not()`、`space-x-*`
之类的兄弟组合器、`@container`）。该规则已被移除。修复方式：
`space-x-*` → `gap-*`；交互状态 → 微信的 `hover-class`；容器查询 →
`@media`。

## M1007 — 未通过 `.value` 使用响应式值

状态、派生和 store 盒子都通过 `.value` 读写——裸引用不会编译出任何
响应式代码，因此被拒绝。frontmatter（带行:列）和模板表达式（包括内联
事件 handler）中都会检查。

```ts
const count = state(0)
count++            // ✗ M1007
count.value++      // ✓
```

```jsx
<span>{count}</span>          // ✗ M1007 — 不渲染任何内容
<span>{count.value}</span>    // ✓
<input value:bind={text} />   // ✓ value:bind 按设计接收盒子本身
```

遮蔽响应式名称的局部变量（例如名为 `count` 的参数）在其自身作用域内
不会被标记。

## M1008 — 缺少 key 的列表（警告）

对响应式数组的 `.map()` 没有 `key=` 会关闭键控字段级 diff：每次更新
都会重发整个数组，而不是按条目路径发送。

```jsx
{items.value.map(t => <li>{t.text}</li>)}            // ⚠ M1008 — 整数组写入
{items.value.map(t => <li key={t.id}>{t.text}</li>)} // ✓ 路径精确
```

## M1009 — 三层及以上嵌套循环中的调用

一层循环内的调用按项提升（`_c` 字段）；两层循环内的调用同样会提升——
生成的派生把外层列表映射为嵌套的已映射列表，内层 `wx:for` 绑定
`_hl<i>[outerIndex]`。三层及以上是提升的边界：派生会在作用域之外捕获
中间层循环变量。请改为在 frontmatter 中预先计算——例如用一个派生把
嵌套项映射为可直接展示的值。

## M1010 —— 模板语法错误

标签不匹配/未闭合以及格式错误的属性名——报告文件行号。常见原因：自闭合
的原生标签没有写 `/>`。

## M1011 — 无效的 `'mist'` 导入

从 `'mist'` 的命名导入会对照真实的导出列表进行校验——`state`、`derived`、
`store`、`props` 和各生命周期钩子。未知名称（通常是 mist 不支持的微信
钩子，或拼写错误）、别名导入（`state as s`）以及默认/命名空间导入都会在
编译时报错，而不是在页面加载时静默失效。

## M1013 — 生命周期钩子用在错误的单元类型中

每个钩子只属于一种单元类型。页面拒绝 `onPageShow`/`onPageHide`（请使用
`onShow`/`onHide`），也拒绝组件专用钩子（`onCreate`、`onMove`）。组件
拒绝页面专用钩子（`onPullDownRefresh`、`onReachBottom`、`onPageScroll`、
`onTabItemTap`、分享/收藏钩子、`onRouteDone`、`onSaveExitState`）——微信
永远不会把它们派发给组件。`app.mist` 只接受 `onLaunch`、`onShow`、
`onHide`、`onError`、`onPageNotFound`、`onUnhandledRejection`、
`onThemeChange`。`onResize` 在页面和组件中都可用
（→ `pageLifetimes.resize`）。

## M1017 — 在 `onCreate` 中写入状态

`onCreate` 映射到微信的 `created`，它在组件实例上的 `properties` 和
`data` 存在**之前**运行——此时由 `setData` 支撑的写入所指向的对象还不
存在。`onCreate` 只应用于初始化非响应式的实例字段（状态之外的
`this._foo = ...`）；把响应式写入移到 `onAttach`。

```ts
import { state, onCreate } from 'mist'
const n = state(0)
onCreate(() => { n.value = 1 })          // ✗ M1017 — created 在 data 存在之前运行
onCreate(() => { console.log(n.value) }) // ✓ 读取没有问题
```

## M1012 — 配置了功能却缺少对应 handler（警告）

设置了 `enablePullDownRefresh: true` 却没有 `onPullDownRefresh`，或设置了
`onReachBottomDistance` 却没有 `onReachBottom`：配置会原样传递，但没有
任何代码能够响应——例如下拉刷新的加载动画永远不会停止。请声明对应钩子，
或删除该配置键。

## M1014 — 配置键与生成字段冲突

单元的 JSON 由用户的 `config` 对象与编译器生成的字段拼接而成——重复的键
会静默地生效或被覆盖（取决于微信的解析器），且没有任何诊断。mistc 转而
直接拒绝这种冲突：

- `app.mist`：`pages`（由 `src/pages/` 生成）、`subPackages`
  （由 `src/packages/` 生成）和 `sitemapLocation`（始终为
  `sitemap.json`）。
- 组件：`component`（mistc 会自动把 `src/components/` 下的每个单元标记
  为组件）。
- 页面和组件：`usingComponents`，**仅当该单元同时导入了 `.mist` 组件
  时**——mistc 会自动注册这些导入，且不会把它们与手动条目合并。

**没有**导入任何 `.mist` 组件的手动 `usingComponents` 仍然可用——这是
手动注册 mistc 无法自行发现的原生/第三方组件的受支持方式。

## M1015 — 无效的插件说明符或插件组件

`plugin://<name>` 导入和 `config.pluginComponents` 条目都会被校验：

- 只支持对整个插件的默认导入——
  `import { x } from 'plugin://calendar'` 报错。
- 插件名必须非空，且只能包含字母数字/`-`/`_`——
  `import p from 'plugin://'` 报错。
- `config.pluginComponents` 的值必须是以 `'plugin://'` 开头的字符串
  字面量。
- `pluginComponents` 名称与导入的 `.mist` 组件标签冲突时报错。

```ts
import cal from 'plugin://calendar'          // ✓
import { x } from 'plugin://calendar'        // ✗ M1015 — 命名导入
import p from 'plugin://'                    // ✗ M1015 — 空名称
```

## M1016 — 子目录中的页面不会被编译（警告）

页面必须直接位于 `src/pages/` 中，或作为分包位于
`src/packages/<pkg>/pages/` 中。位于 `pages/` 下其他任何位置的 `.mist`
文件都会被静默跳过——此警告会告诉你具体位置。

```
src/pages/sub/extra.mist    // ✗ M1016 — 被丢弃，既不是页面也不是分包
src/pages/index.mist        // ✓ 主包页面
src/packages/shop/pages/cart.mist   // ✓ 分包页面
```

## M1018 — `text` 映射元素内出现盒式样式的子元素（警告）

原生 `text` 只以内联方式渲染，会忽略盒式样式（padding、flex 等）。映射
到 `text` 的元素（`span`，或字面的 `text` 标签）只能包含文本/`{expr}`
子节点或其他映射到 `text` 的元素——其他任何内容（`view`、`image`……）
都能正常编译，但其盒式样式会被静默忽略。

```jsx
<span class="p-4 flex"><div>x</div></span>   // ⚠ M1018 — div 的 padding/flex 被忽略
<span>hi {name.value}</span>                 // ✓
<span><span>nested</span></span>             // ✓
```

该检查会递归穿过同一位置的 `wx:if`/`wx:else` 和列表子节点，因此 `span`
内的条件或循环盒式元素同样会触发警告。

## M1019 — 未知标签（警告）

一个既不是微信原生组件、也不是 Web 别名（`div`、`span`、`img`……）、也
不是已注册的 `.mist` 组件/插件组件/手动 `usingComponents` 条目的标签能
正常编译，但不会渲染任何内容——微信会静默丢弃无法识别的标签，自己不报
任何错误。

```jsx
<scroll-veiw>x</scroll-veiw>   // ⚠ M1019 — 你是想写 <scroll-view> 吗？
<swipper />                    // ⚠ M1019 — 你是想写 <swiper> 吗？
<scroll-view>x</scroll-view>   // ✓ 原生
```

只有当某个原生标签或 Web 别名的编辑距离不超过 2 时才会给出建议；否则
警告会省略「did you mean」部分。每个不同的未知标签在每个单元中只警告
一次，无论出现多少次。

如果该标签是有意为之——一个只通过 `config.usingComponents` 注册的第三方
组件，或你暂时不希望 mistc 知道的组件——把它列入 `config.customTags`
即可抑制该警告：

```ts
export const config = { customTags: ['my-web-component'] }
```

`customTags` 条目只能包含字母、数字、`-` 和 `_`；该键在编译时被消费，
永远不会进入生成的 `.json`。

## M1020 — 自定义 tab bar 的文件/配置不匹配

微信要求自定义 tab bar 组件位于固定的输出路径 `custom-tab-bar/index.*`。
当 `src/custom-tab-bar.mist` 存在时，mistc 会把它编译到该位置。文件与
`tabBar.custom: true` 配置标志必须保持一致：

```ts
// tabBar.custom: true, but src/custom-tab-bar.mist is missing
export const config = { tabBar: { custom: true, list: [...] } }
// ✗ M1020（错误）——微信会渲染一个空白的 tab bar
```

```ts
// src/custom-tab-bar.mist exists, but config lacks tabBar.custom: true
export const config = { tabBar: { list: [...] } }
// ⚠ M1020（警告）——微信会忽略该文件并渲染内置 tab bar；构建仍会成功
```

只要 `src/custom-tab-bar.mist` 存在，就在 `app.mist` 的 config 中设置
`tabBar: { custom: true, ... }`；否则删除该文件（或该标志）。

## M1021 — 未知的 navigate() 路由

`navigate(route)`、`navigate.replace(route)` 和 `navigate.switchTab(route)`
要求路由为字符串字面量，因为编译器会用它对照编译出的页面列表进行检查。
标识符、带插值的模板字符串或字符串拼接都会以同一条消息报错：

```ts
navigate('/pages/index/index')          // ✓ 字面量
navigate(`/pages/index/index`)          // ✓ 字面量（无插值）
navigate(someVar)                       // ✗ M1021 — 不是字面量
navigate(`/pages/${id.value}`)          // ✗ M1021 — 不是字面量
navigate('/pages/' + id.value)          // ✗ M1021 — 不是字面量
```

不在编译页面列表中的字面量路由同样会报错；当某个已知路由的编辑距离
不超过 3 时会附带建议：

```ts
navigate('/pages/abot/abot')
// ✗ M1021: unknown route '/pages/abot/abot' — not in the compiled page
//   list; did you mean '/pages/about/about'?
```

当 `app.mist` 的 `config.tabBar.list[].pagePath` 可静态提取（每个条目都
是普通字符串字面量）时，`navigate.switchTab(route)` 还额外要求该路由是
一个 tab-bar 页面；否则回退到上面的普通路由列表检查。

路由校验只在目录构建（`mistc build <dir>`）中运行，因为只有目录构建拥有
完整的页面列表——`mistc build <file>`（平铺/单入口构建）编译
`navigate()` 调用时不会对路由做任何检查。

## 值得了解的无编码错误

- **"store modules require a project build"**——相对路径的非 `.mist`
  导入只在目录构建（`mistc build <dir>`）中解析。
- **"plain values cannot cross the page boundary"**——store 模块只能导出
  `store()` 值和函数；请导出一个 getter 函数而不是 const。
- **"app.mist cannot declare state / have a template"**——应用壳只包含
  生命周期 + 配置 + 全局样式。
- **"config must be a static object literal"**——`export const config`
  中不能有函数调用或变量。
- **store 输出路径冲突**——两个 store 文件的主名相同；重命名其中一个。
- **"TS enum … is not supported"**——enum 是运行时构造；请使用 const
  对象或字符串字面量联合类型。
- **"npm packages are not supported"**——只能导入 `mist`、相对路径的
  store 模块和 `.mist` 组件。

## 静默隐患——M1001 看不到的别名写入

M1001 能捕获通过单次赋值本地别名的直接写入。它无法追踪的写入——传入会
变更其参数的辅助函数的别名，或对派生副本的回调内部的变更——仍然不会
编译出任何响应式代码。请始终通过完整的 `x.value…` 路径写入。

## M1022 — 由 frontmatter 代码初始化的模板绑定状态

调用 frontmatter 函数（或读取其他状态）的 `state()` 初始化器无法为
`data` 提供初始值——`data: {}` 字面量在任何页面代码运行之前求值，因此
那里不存在 `this`。未绑定到模板的状态没有问题：它在 `onLoad` 中完成
初始化，此时该调用会编译为方法调用。

```ts
function generate() { return [1, 2, 3] }

const items = state(generate())
```

```jsx
<span>{items.value.length}</span>          // ✗ M1022 — items 已绑定到模板
```

修复方式：预先计算为模块级 const（在任何函数之外的
`const INITIAL = [...]`），或让状态保持未绑定，并通过一个 `derived` 来
渲染它：

```ts
const INITIAL = [1, 2, 3]
const items = state(INITIAL)               // ✓ const 初始值在任何地方都可用
```

## M1023 —— 原生标签上的未知事件

原生标签上的 `onXxx` 会盲目编译为 `bindxxx`——而微信会静默忽略它不认识的
事件，所以打错字的处理函数永远不会触发。编译器元数据表中的标签（约 25 个
日常组件）会检查事件；表外的标签完全跳过。

```jsx
<scroll-view onScrolToLower={more} />   // ✗ M1023 —— 是想写 onScrollToLower 吗？
<scroll-view onScrollToLower={more} />  // ✓
```

你确定存在的自定义事件（较新的基础库、自渲染标签）可以用
`config.customAttrs = ['onMyEvent']` 放行。

## M1024 —— 原生标签上的未知属性

与 M1023 同一类静默失败：微信忽略它不认识的属性，所以 `scrol-y` 只是
无声无息地不生效。只有元数据表中的标签会检查属性；`data-*`、`aria-*`、
带命名空间的属性（`class:list`、`value:bind`、`mark:*`）和通用属性
（`class`、`style`、`id`、`hidden`、`hover-*` 等）总是放行。

```jsx
<scroll-view scrol-y />    // ✗ M1024 —— 是想写 scroll-y 吗？
<scroll-view scroll-y />   // ✓
```

表里还不认识的属性（微信每季度都会新增）可以用
`config.customAttrs = ['the-new-attr']` 放行——过期永远不会破坏构建；
两个代码都只是警告。

## M1025 —— 路由参数页面缺少对应的 state

`pages/<dir>/[<param>].mist` 路由页面必须把参数声明为 state——编译器负责
从查询参数注入并守卫缺参，但声明由你来写（它确定类型和初始值）：

```ts
// pages/item/[id].mist
const id = state('')          // ✓ 在 onLoad 运行前从查询参数注入
```

没有它，参数无处存放——报 M1025，并附上要添加的确切声明。查询参数是
字符串；需要数字时在 `derived` 里转换。

## M1026 —— 响应式值被传给 npm 导入

npm 导入是受支持的，但它是**不透明边界**：编译器把库打包进来却无法看进
它的内部，因此作为参数传入的响应式值（state、derived 或 store 镜像）
可能被不可见地变更——这正是编译器要消灭的那类静默过期问题。

```ts
import { format } from 'date-fns'
const when = state({ ts: 0 })

format(when.value, 'yyyy')          // ✗ M1026 —— 响应式对象越过了边界
const ts = when.value.ts
format(ts, 'yyyy')                  // ✓ 普通局部拷贝进去，普通值出来
format(raw(when.value), 'yyyy')     // ✓ 已确认边界——调用后 when 被重新同步
```

两种修法。优先用普通拷贝。当库必须拿到实时值时，用 `raw()`（从
`'mist'` 导入）包裹：包装在编译后消失，编译器在调用后保守地重新同步
整个被包裹的根——对该字段做一次全量 `setData`（未绑定的 state 则重新
计算 derived），因此每次调用都要付出一次完整序列化的成本。

返回值是普通数据——随意赋给 state。检查覆盖直接调用，包括成员调用
（`dayjs.utc(...)`）；通过别名或回调转交响应式值属于 M1001 记录的同一类
不可追踪边界。

## M1027 —— 特性超出 config.minLibVersion

可选检查：声明 `config.minLibVersion`（即你在微信管理后台设置的最低基础
库版本）——在 `app.mist` 声明一次即全项目生效，单元可自行覆盖——编译器会把每个有文档最低版本的已用原生特性与它对比：

```ts
export const config = { minLibVersion: '2.9.0' }
```

```jsx
<scroll-view refresher-enabled />   // ✗ M1027 —— refresher-enabled 需要 ≥ 2.10.1
<input value:bind={text} />         // ✗ M1027 —— value:bind 需要 ≥ 2.9.3
```

修复方式：调高 `minLibVersion`（连同后台设置）或去掉该特性。版本表是
人工整理且刻意不完整的——没有记录最低版本的特性永远不检查；不设置
`minLibVersion` 则完全不做版本检查。警告级：过期永远不会破坏构建。

## M1028 —— 打包的 npm 包引用了浏览器 API

每个 vendor 产物都会被扫描微信 JS 运行时没有的全局对象——`window`、
`document`、`navigator`、`localStorage`、`sessionStorage`、
`XMLHttpRequest`。命中意味着这段代码在真机上执行到时会抛错：

```
M1028: npm package 'domish' references window, document — these APIs don't
exist in WeChat's JS runtime and fail when reached
```

扫描是词法启发式的：裸存在性检查（`typeof window !== 'undefined'`）不会
命中，但防御性的成员读取（`if (window.matchMedia)`）仍会命中。确认包能
安全降级后，在 `app.mist` 中放行（该键仅限 app 级，其他位置声明会报错）：

```ts
export const config = { trustedPackages: ['fuse.js'] }
```

警告级：无论如何构建都会成功。

## M1029 —— 包体积超出限制

每次项目构建结束时都会打印体积摘要
（`size: main 1.2MB, shop 340KB, total 1.5MB`），并在包体积越过微信的
上传限制时告警——主包 2MB、每个分包 2MB、总计 20MB：

```
M1029: main package is 2.31MB — exceeds WeChat's per-package limit (2.00MB)
```

在 `app.mist` 中声明 `config.sizeBudget`（仅限 app 级，其他位置声明会
报错）可以按你自己的每包阈值提前告警：

```ts
export const config = { sizeBudget: '1.5MB' }   // 或 '800KB'
```

统计的是生成文件的字节数之和；微信度量的是上传后的包，因此数字接近但
不精确到字节——这也是它只是警告级的原因。修复方式：把页面移入
`src/packages/` 分包、精简 npm vendor 体积，或压缩 `assets/`。

## M1030 —— 在 function() 回调中写状态

非箭头函数（`function` 表达式、`success(res) { ... }` 这类方法简写）会
重新绑定 `this`，编译后的写入无法到达页面状态：

```ts
wx.request({
  url: '/api',
  success(res) { items.value = res.data },      // ✗ M1030 —— 这里的 `this` 是 options 对象
  // success: (res) => { items.value = res.data }  ✓ 箭头函数保留页面的 `this`
})
```

请使用箭头函数。handler 内嵌套的 `function` 声明同样适用。
