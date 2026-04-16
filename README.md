# P2P Chat

基于 `Rust + Tauri 2 + Vue 3 + PrimeVue + TypeScript` 的桌面端聊天应用重构工程。

前端包管理统一使用 `pnpm`。

## 快速开始

```bash
pnpm install
pnpm tauri:dev
```

仅构建前端：

```bash
pnpm build
```

仅检查 Rust 原生层：

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

## 主要文件

- `Project.md`: 项目定位、架构分层、里程碑和开发约束。
- `Agent.md`: 协作规范、目录职责和交付要求。
- `BuildProgress.md`: 当前阶段、已完成事项和下一步计划。
