# Project.md

## 项目定位

`P2P Chat` 是一个基于 `Rust + Tauri 2 + Vue 3 + PrimeVue + TypeScript` 的桌面端聊天应用重构工程。当前阶段的首要目标不是一次性补齐全部业务，而是建立稳定、清晰、可扩展的桌面端基础架构。

## 核心目标

1. 建立跨平台桌面应用基础壳层。
2. 明确前端界面层与 Rust 原生能力层的职责边界。
3. 为后续会话管理、消息同步、P2P 连接、本地存储和打包发布预留扩展结构。
4. 保持初始化阶段简单可控，避免过早引入不必要的复杂度。

## 技术栈

- 桌面容器: `Tauri 2`
- 原生层: `Rust`
- 前端框架: `Vue 3`
- UI 组件库: `PrimeVue`
- 图标库: `PrimeIcons`
- 构建工具: `Vite`
- 语言: `TypeScript`
- 包管理: `pnpm`

## 架构分层

### 前端层 `src/`

- 负责桌面窗口内界面渲染、交互流程、状态组织和组件编排。
- 通过 Tauri 命令调用 Rust 原生层能力。
- 以 PrimeVue 作为基础组件层，自定义样式用于贴合聊天产品视觉结构。

### 原生层 `src-tauri/`

- 负责桌面窗口能力、命令注册、系统资源访问和跨平台封装。
- 承载后续聊天域服务、网络能力、持久化与性能敏感逻辑。
- 对前端暴露稳定、明确的命令接口，而不是让前端直接感知内部实现细节。

## 推荐目录规划

```text
src/
  components/
  features/
  mock/
  services/
  types/
  App.vue
  main.ts

src-tauri/
  src/
    bin/
    commands/
    app/
    domain/
    infra/
    lib.rs
    main.rs
```

说明:

- `components/` 放通用 UI 组件。
- `features/` 按业务模块拆分功能，例如会话、消息、联系人、设置。
- `mock/` 放初始化阶段的演示数据和结构样例。
- `services/` 放前端适配层，例如 Tauri 调用封装、格式转换和数据访问入口。
- `src-tauri/src/bin/` 放 companion native binaries，例如桌面预览 transport runtime。
- `domain/` 放 Rust 侧的领域对象与业务规则。
- `infra/` 放存储、网络、平台适配等实现细节。

## 里程碑规划

### Phase 0: Foundation

- 搭建 `Tauri + Vue + PrimeVue` 工程骨架。
- 确立文档与协作约定。
- 用桌面端壳层替换默认模板内容。

### Phase 1: App Shell

- 设计页面路由、导航和设置入口。
- 拆分会话列表、聊天主窗和资料面板组件。
- 建立前端服务层骨架。

### Phase 2: Chat Domain

- 定义用户、会话、消息等核心类型。
- 建立前端 / Rust DTO 与命令接口。
- 形成最小可演示的聊天流程。

### Phase 3: P2P Transport

- 抽象连接管理、节点发现、消息传输与错误恢复机制。
- 明确网络层与聊天域的边界。
- 为日志与可观测性预留接口。

### Phase 4: Persistence and Packaging

- 接入本地存储与配置持久化。
- 补充桌面平台打包、版本信息和发布脚本。
- 完成基础质量门禁。

## 开发命令

```bash
pnpm install
pnpm check
pnpm version:check
pnpm version:set 0.1.1
pnpm native:check
pnpm native:test
pnpm verify
pnpm desktop:build
pnpm desktop:build:full
pnpm release:linux
pnpm release:prepare 0.1.1
pnpm tauri:dev
pnpm build
```

## 当前约束

1. 初始化阶段先保证结构清晰与命令可运行，不急于补齐全部业务能力。
2. 正式文档、源码和构建流程只围绕当前仓库展开，不引入临时目录路径依赖。
3. 本地 preview runtime 必须可在仓库内直接构建和解析，不能只依赖全局安装命令。
4. 每次完成阶段性工作后，都要同步更新 `BuildProgress.md`。
