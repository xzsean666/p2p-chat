# P2P Chat

基于 `Rust + Tauri 2 + Vue 3 + PrimeVue + TypeScript` 的桌面端聊天应用重构工程。

前端包管理统一使用 `pnpm`。

## 快速开始

```bash
pnpm install
pnpm tauri:dev
```

前端类型检查：

```bash
pnpm check
```

检查版本号是否在 `package.json / Cargo.toml / tauri.conf.json` 中保持一致：

```bash
pnpm version:check
```

统一更新桌面端版本号：

```bash
pnpm version:set 0.1.1
```

仅构建前端：

```bash
pnpm build
```

仅检查 Rust 原生层：

```bash
pnpm native:check
```

运行 Rust 原生层测试：

```bash
pnpm native:test
```

说明:

- `pnpm native:test` 会同时编译 companion binary `p2p-chat-runtime`。
- 预览模式下的本地 runtime 命令解析优先查 `PATH`，其次回退到仓库内 `src-tauri/target/debug` 或 `src-tauri/target/release` 的本地构建产物。

执行完整校验：

```bash
pnpm verify
```

执行桌面端打包：

```bash
pnpm desktop:build
```

说明:

- `pnpm desktop:build` 默认生成当前环境已验证通过的 Linux 包类型 `deb` 与 `rpm`。
- 若需要尝试完整 bundler 目标，可执行 `pnpm desktop:build:full`。
- 若需要整理发布目录并生成校验和、清单和发布说明，可执行 `pnpm release:linux`。
- 若需要一键完成版本同步、Linux 发版产物整理和 changelog 模板生成，可执行 `pnpm release:prepare 0.1.1`。

## 主要文件

- `Project.md`: 项目定位、架构分层、里程碑和开发约束。
- `Agent.md`: 协作规范、目录职责和交付要求。
- `BuildProgress.md`: 当前阶段、已完成事项和下一步计划。
- `NextSession.md`: 给下一次新 session 的交接上下文、验证基线和建议切口。
