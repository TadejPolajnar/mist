# Mist 文档

[English → README.md](README.md)

Mist 是一门**面向微信小程序的组件语言与编译器**：你编写 Astro 风格的 `.mist`
单文件组件，`mistc`（Rust）把它编译为普通的 WXML/WXSS/JS，核心是路径精确的
`setData` 和一个约 9 KB 的运行时。

本文档描述的是**今天已经实现的功能**。完整语言设计见
[../SPEC.md](../SPEC.md)——其中一部分仍处于设计阶段；两者不一致时，以本文档为准。

- **[快速上手](#快速上手)**（就在下方）
- **[语言指南](language.zh-CN.md)**——文件结构、响应式、模板、组件、store、样式
- **[API 参考](api.zh-CN.md)**——`'mist'` 可导入的全部内容、命令行、产物说明
- **[测试](testing.zh-CN.md)**——`mistc test`：在 Node 中启动编译后的页面，对状态与 `setData` payload 大小断言
- **[诊断说明](diagnostics.zh-CN.md)**——每一个 `M` 编码及修复方法

## 快速上手

环境要求：Rust、Node.js + npm（Tailwind 与测试需要）、微信开发者工具。

```sh
git clone https://github.com/TadejPolajnar/mist.git && cd mist
cargo install --path . && cargo install --path crates/mistc-lsp   # 把 mistc 和 mistc-lsp 装进 PATH
```

脚手架、构建、迭代：

```sh
mistc init my-app              # app.mist + 一个待办页面 + 一个示例测试 + project.config.json
cd my-app
mistc build src --watch        # 每次保存自动重编译；ctrl-c 退出
mistc test                     # 在 Node 测试环境中运行 tests/*.test.js
# 微信开发者工具 → 导入项目 → 选择 my-app/
```

`mistc --help` / `mistc build --help` 有全部参数说明。

也可以手动创建文件：

```
my-app/
├── app.mist
└── pages/
    └── index.mist
```

`app.mist`——应用生命周期、全局配置、全局样式（无模板、无状态）：

```
---
import { onLaunch } from 'mist'
export const config = { window: { navigationBarTitleText: '我的应用' } }
onLaunch(() => console.log('launched'))
---
```

`pages/index.mist`：

```
---
import { state } from 'mist'
export const config = { navigationBarTitleText: '计数器' }

const count = state(0)

function inc() {
  count.value++
}
---
<div class="p-4 flex flex-col gap-2">
  <span class="text-2xl font-bold">{count.value}</span>
  <button class="rounded-full bg-blue-500 text-white" onTap={inc}>+1</button>
</div>
```

构建与运行：

```sh
mistc build my-app -o dist            # 单次构建
mistc build my-app -o dist --watch    # 保存即重编译
# 微信开发者工具 → 导入项目 → 选择包含 project.config.json 的目录
# （其中 "miniprogramRoot": "dist/"；mistc init 已为你生成）
```

保存 → 自动重编译 → 开发者工具重新编译预览。这就是完整的开发循环。

## 接下来读什么

功能最全的示例是 [`examples/food`](../examples/food)——一个 6 页的点单小程序
（持久化购物车、规格选择、分享钩子、原生风格组件）。用
`mistc build examples/food/src -o examples/food/dist` 编译，然后在开发者工具中
导入 `examples/food/`。

从头到尾快速浏览 [language.zh-CN.md](language.zh-CN.md)——篇幅不长，覆盖了现有的全部功能。
第一次被编译器拒绝时，把 [diagnostics.zh-CN.md](diagnostics.zh-CN.md) 打开放在手边；
每个错误都写明了修复方法。
