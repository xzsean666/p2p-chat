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

## Android APK

当前 `scripts/install-android-build-deps.sh` / `scripts/build-android-apk.sh` 主要支持 **Debian / Ubuntu**（依赖安装脚本会校验系统类型）。

最小可执行流程（Debian/Ubuntu）：

```bash
# 1) 安装 Android + Rust + Node/pnpm 构建依赖
pnpm android:deps

# 2) 安装项目前端依赖
pnpm install --frozen-lockfile

# 3) 导出环境变量（按本机实际路径调整）
export ANDROID_HOME="$HOME/Android/Sdk"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export PATH="$ANDROID_SDK_ROOT/platform-tools:$ANDROID_SDK_ROOT/cmdline-tools/latest/bin:$PATH"
source "$HOME/.cargo/env"

# 4) 构建默认 APK（默认 target: aarch64）
pnpm android:apk
```

多 target / 产物示例：

```bash
# 同时构建多个 ABI
pnpm android:apk -- --target aarch64 armv7 x86_64

# 使用等号形式传 target（脚本支持 --target=...）
pnpm android:apk -- --target=aarch64,x86_64 --split-per-abi

# 构建 AAB
pnpm android:apk -- --aab --target aarch64
```

说明:

- `pnpm android:apk` 会自动检查 `pnpm`、`node`、`cargo`、`rustup`、`java`、`javac`、`sdkmanager`。
- Android SDK 路径优先读取 `ANDROID_SDK_ROOT` / `ANDROID_HOME`，否则回退到 `$HOME/Android/Sdk`。
- 首次构建若未初始化 Android 工程，脚本会自动执行 `pnpm tauri android init --ci --skip-targets-install`。
- 当前仓库已补齐 APK 构建链路脚本，但 Android 端聊天 runtime 仍是桌面预览架构，真实聊天能力还不能视为 Android 已完成适配。

## 主要文件

- `Project.md`: 项目定位、架构分层、里程碑和开发约束。
- `Agent.md`: 协作规范、目录职责和交付要求。
- `BuildProgress.md`: 当前阶段、已完成事项和下一步计划。
- `NextSession.md`: 给下一次新 session 的交接上下文、验证基线和建议切口。
