---
paths:
  - "src/**"
  - "*.ts"
  - "*.vue"
  - "index.html"
---

# 前端开发规则（Vue 3 + TS + Pinia + shadcn-vue）

## 组件写法

- 一律使用 Vue 3 组合式 API：`<script setup lang="ts">`，禁止 Options API。
- 组件文件 PascalCase（`RepoRow.vue`），一个文件一个组件。
- 业务逻辑（数据获取、状态变更）写进 Pinia store 的 actions，组件只负责调用 action 和渲染，禁止在组件里写复杂业务逻辑。

## 目录职责

| 目录 | 职责 | 规则 |
|---|---|---|
| `src/components/` | 业务组件 | 手写组件放这里 |
| `src/components/ui/` | shadcn-vue 基础组件 | 由 `pnpm dlx shadcn-vue@latest add <组件>` 生成，**不要手改内部实现**；样式定制在业务组件层用 props/class 完成 |
| `src/stores/` | Pinia store | 全局状态唯一来源 |
| `src/lib/invoke.ts` | Tauri command 封装 | 全项目唯一 invoke 出口 |
| `src/lib/types.ts` | 与 Rust serde 对齐的类型 | 改动必须与 `src-tauri/src/config.rs` 同步 |

## Tauri 调用（强制）

- **禁止**在组件/store 中直接 `import { invoke } from "@tauri-apps/api/core"`；必须调用 `src/lib/invoke.ts` 中已封装的函数。
- 新增 command 时：先在 Rust 侧实现并注册，再在 `invoke.ts` 加一个带 TypeScript 返回类型的封装函数，store/组件只消费这个函数。
- invoke 参数名用 camelCase（Tauri 2 自动映射 Rust 侧 snake_case 参数），如 `invoke("save_settings", { scanRoots, maxDepth })`。

## 类型与数据契约

- 所有跨端数据结构的字段名保持 **snake_case**（与 Rust serde `rename_all = "snake_case"` 对齐），如 `setup_done`、`scan_roots`、`max_depth`，不要改成 camelCase。
- 枚举用字符串字面量联合类型（如 `type Direction = "github_to_gitee" | "gitee_to_github" | "both"`），取值必须与 Rust 侧 serde 序列化结果一致。
- 禁止 `any`；与后端交互的数据必须有显式类型。

## 样式

- Tailwind CSS 原子类为主；条件合并 class 用 `src/lib/utils.ts` 的 `cn()`（clsx + tailwind-merge）。
- 图标用 `lucide-vue-next`，按需命名导入。
- 主题色沿用现有 shadcn 配置（`tailwind.config.ts` + `src/styles.css` 中的 CSS 变量），不要引入新的颜色体系。

## 错误处理与 UX

- 所有 invoke 调用都可能 reject：store action 内 catch 后写入状态（如 `scanError`）供 UI 展示，禁止静默吞错。
- 错误文案要**可操作**，例如「gitee push 被拒：需要先完成一次终端 push 以保存凭据」，而不是裸透 err 字符串。
- 加载/扫描等异步过程要有可见的进行中状态（`scanning` 等），完成后再更新数据。

## 自检

改动前端代码后运行 `pnpm build`（含 `vue-tsc --noEmit` 类型检查）确认无类型错误。
