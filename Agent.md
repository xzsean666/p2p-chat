# Agent.md

## 目标

本仓库用于构建 `P2P Chat` 桌面端应用，技术栈固定为 `Rust + Tauri 2 + Vue 3 + PrimeVue + TypeScript`。

## 当前阶段硬约束

1. 当前阶段唯一 P0 目标是: `通过 Nostr relay 完成真实聊天`，优先打通 `登录/凭据 -> signer/runtime -> relay publish -> inbound sync -> 消息展示/回执` 这条主链。
2. 凡是需要独立后端、圈子服务端、对象存储服务、付款/订阅服务或任何额外服务端接口配合的任务，一律暂停，除非用户明确重新开启。
3. 暂停范围至少包括:
   - `file server` 管理页与对应数据模型迁移
   - `NIP-96 / Blossom / MinIO / S3 / presigned URL / paid circle S3 config`
   - 任何 `upload / download / media backend` 扩展
   - `purchase / subscription / private cloud / paid relay backend`
   - `push / share / scan / permission / tor` 中依赖服务端协作的部分
4. 若某项功能必须依赖后端才能成立，不要继续扩展该方向，也不要临时发明新的 mock backend、env backend 或服务端合同；应明确标记为 `deferred`，转而继续推进 Nostr 聊天主链。
5. 当前允许继续做的重点只包括:
   - 本地 `nsec / hexKey` 登录与签名
   - `bunker / NIP-46` 远端 signer 握手与签名
   - relay 连接、发布、订阅、增量同步、回执与去重
   - Nostr direct/group message 映射、持久化与会话展示
   - 不依赖后端的消息域模型收敛与错误处理
6. `Nostr 聊天主链` 的完成标准里，不接受“必须用户手动进入 diagnostics 或先点一次 `Connect` 才能工作”这种前提；登录后、选圈后、建圈后，runtime auto-start / auto-recover 必须默认成立。

## 协作原则

1. 以当前仓库为唯一正式工程，不把临时目录、外部代码路径或一次性调试资源写入正式文档、构建脚本或源码依赖。
2. 保持前端界面层与 Rust/Tauri 引擎层边界清晰：界面和交互状态放在 `src/`，桌面能力、命令桥接、系统访问和后续领域逻辑放在 `src-tauri/`。
3. UI 基础优先使用 PrimeVue 组件与可控样式扩展，不随意引入额外 UI 框架。
4. 前端包管理固定使用 `pnpm`，锁文件和脚本命令统一围绕 `pnpm` 维护，不保留 `npm` 锁文件。
5. 先完成最小可运行闭环，再逐步接入真实聊天域模型、P2P 能力和本地持久化；当前闭环以 `Nostr 聊天主链` 为唯一优先级，不扩展后端依赖功能。
6. 发生架构调整、阶段推进或开发流程变化时，同步更新 `BuildProgress.md` 和相关项目文档。
7. 这是一个重构项目；涉及现有功能页重做时，优先对齐 `tmp/xchat-app-main` 的信息架构、交互节奏和视觉层级，不要先做自定义大改版，尤其是 `login / onboarding / circle entry`。
8. 除非用户明确要求做 GUI 运行验收，否则不要依赖不可控的桌面自动点按、自动截图或无监督界面操作来下结论；优先做源码对照、静态实现和可复现的构建/测试验证。

## 目录约定

- `src/`: Vue 应用入口、页面壳层、组合式逻辑、Mock 数据和前端服务。
- `src/components/`: 可复用的界面组件。
- `src/features/`: 业务模块，例如会话、消息、联系人、设置。
- `src/services/`: Tauri 命令封装、数据访问适配、前端服务层。
- `src/types/`: 共享类型定义。
- `src-tauri/src/`: Rust 原生入口、命令注册、应用服务与基础设施实现。
- `src-tauri/src/bin/`: companion native binaries，例如预览模式使用的 `p2p-chat-runtime`。

## 编码要求

1. Vue 组件保持展示层与业务层分离，复杂状态优先拆到组合式函数或服务层。
2. Rust 命令只暴露稳定接口，不在命令层堆积业务细节。
3. 命名以清晰可读为先，避免上下文不明的缩写。
4. 常用开发、校验和打包入口统一走 `pnpm`，至少持续保证 `pnpm version:check`、`pnpm build`、`pnpm native:test`、`pnpm verify`、默认稳定打包入口 `pnpm desktop:build`、Linux 发布整理入口 `pnpm release:linux` 以及一键发布准备入口 `pnpm release:prepare <version>` 可用。
5. 本地 preview runtime 相关改动要同时维护 companion binary、launch arguments 与本地命令解析回退，不允许只假设命令已经在全局 `PATH` 中存在。
6. 若任务选择出现冲突，优先级固定为:
   - `Nostr 文本聊天主链`
   - `Nostr signer/runtime 稳定性`
   - `消息同步/回执/持久化`
   - 其他全部延后，尤其是任何后端依赖能力

## 完成定义

一次有效交付至少满足以下条件：

1. 代码或文档与当前阶段目标一致。
2. 已执行对应的构建或检查命令，或明确说明阻塞原因。
3. 若影响架构、里程碑或开发流程，已同步更新文档。
