# BuildProgress

更新时间: 2026-04-16

## 当前阶段

- 阶段名: `Phase 0 / Foundation`
- 状态: `In Progress`
- 目标: 完成桌面端基础工程、PrimeVue 界面壳和文档基线。

## 已完成

- [x] 初始化 `Tauri 2 + Vue 3 + TypeScript` 项目骨架。
- [x] 接入 PrimeVue 与 PrimeIcons，并建立基础主题配置。
- [x] 将默认模板页替换为接近 XChat 的会话列表 + 聊天主窗。
- [x] 建立 Rust 到 Vue 的基础命令桥接示例。
- [x] 生成项目说明文档、协作文档和进度文档。
- [x] 补齐启动页、Home 顶栏、circle 切换层、设置抽屉和新建消息弹层。
- [x] 补齐会话操作、归档会话弹层和联系人/群详情抽屉。
- [x] 补齐登录页和独立联系人/群资料页。
- [x] 接入前端状态持久化，并通过 Rust/Tauri 命令桥保存聊天壳状态。
- [x] 将归档会话、联系人详情和群资料切换为页面式覆盖层，靠近原应用导航方式。
- [x] 将新建消息改为页面式入口，并补齐找人页、邀请链接复制和联系人筛选流。
- [x] 将 circle 列表切到可持久化状态，并补齐独立 circle 管理页与新增 relay 流。
- [x] 将设置抽屉条目接成真实详情页，并为偏好/通知/高级设置接入本地持久化。
- [x] 补齐 circle 详情页，并接入 circle 编辑、删除、统计信息和多入口详情导航。
- [x] 抽离桌面壳状态到 `features/shell` composable，建立前端 shell 服务层骨架。
- [x] 建立 Rust 侧 `commands / app / domain / infra` 骨架，并补齐聊天 DTO 与 `load_chat_seed` 命令桥接。
- [x] 将前端默认聊天数据改成 `Rust chat seed -> mock fallback` 链路，并移除对旧 `xchatClone` 状态种子的直接依赖。
- [x] 将默认聊天种子进一步拆成 `circles / contacts / sessions / groups / messages` 分命令读取，前端按域组装初始 shell。
- [x] 为 Rust 聊天读取链路补齐 `ChatRepository` 抽象、seed 仓储实现和应用层 query service，命令不再直接拼数据。
- [x] 将聊天域读写仓储切到 SQLite，并补齐 `save_chat_domain_seed` 命令与前端双写持久化链路。
- [x] 为发消息、会话操作、联系人拉起会话和圈子增删改补齐 Rust/Tauri 变更命令，前端优先走真实命令再回退本地状态。
- [x] 建立 P2P transport 抽象骨架、mock diagnostics service 和设置页/圈子详情页的 transport 状态展示。
- [x] 为 relay connect / disconnect / sync 补齐 transport mutation pipeline，并将 Circle 详情页按钮接到真实 Tauri 命令。
- [x] 为 transport snapshot 补齐 peer discovery / session sync 视图，并接通 Circle 详情页的发现与同步动作。
- [x] 为 transport cache 建立独立 SQLite 仓储，并将 peer/session sync 状态切到持久化缓存合并链路。
- [x] 为 transport service 拆出 `websocket / mesh / invite` adapter 层，并补齐协议分支单测。
- [x] 为 transport 再拆出独立 engine 抽象，service 只保留 SQLite 仓储编排与 snapshot 输出。
- [x] 为 transport 补齐 engine factory、`nativePreview` 占位 engine 和 snapshot engine 标识。
- [x] 抽离 transport state builder，并让 `nativePreview` 直接基于共享 builder 构建状态，不再依赖 mock engine 实现。
- [x] 将 transport 本地编排服务命名调整为 `LocalTransportService`，避免与远程后端概念混淆。
- [x] 为 transport snapshot 增加 runtime activity 流，并接通 SQLite cache、浏览器 fallback 和 Circle 详情页时间线展示。
- [x] 将 transport snapshot 进一步接入 Circle 目录页、设置高级页和 About 页，并补齐前端 heartbeat 刷新。
- [x] 将前端包管理统一切到 `pnpm`，同步更新 Tauri build hook、仓库元数据和项目文档。

## 进行中

- [ ] 规划应用路由、布局壳和状态组织方式。
- [ ] 定义聊天域模型与前端 / Rust 引擎数据边界。
- [ ] 抽象 P2P 通信接口。

## 下一阶段

- [ ] Phase 1: App Shell
- [ ] Phase 2: Chat Domain
- [ ] Phase 3: P2P Transport
- [ ] Phase 4: Persistence and Packaging

## 备注

- 当前仓库已经完成基础视觉层、桌面布局和工程初始化。
- 当前桌面壳状态支持优先写入本地 SQLite，浏览器预览环境自动回退到 `localStorage`。
- 当前 circle 已支持从设置抽屉和目录页进入独立详情页，并在壳内完成名称/描述更新与安全删除。
- 当前 `App.vue` 已收敛为组件装配层，主要壳状态和交互已下沉到 `src/features/shell/`。
- 当前默认聊天种子已支持从 Rust/Tauri 命令加载，前端本地 mock 作为浏览器和异常回退。
- 当前前端展示常量、空壳默认状态和聊天 mock 数据已完成拆分，聊天 seed 不再和 UI 文案混在同一文件。
- 当前前端已从整包 seed 读取切到按域读取，为后续接真实会话仓储和消息仓储预留了稳定接口。
- 当前 Rust 读取链路已形成 `commands -> app -> infra -> domain` 的稳定分层，后续可直接替换 seed 仓储为真实存储实现。
- 当前聊天域表与壳状态表都已接入 SQLite，前端变更会同步写回聊天表和 `app_kv`，并保留一次旧 `shell-state.json` 自动迁移。
- 当前会话发送、会话归档/静音/删除、联系人拉起会话和圈子管理已优先通过 Rust 命令更新 SQLite，再将结果回灌到 Vue 壳状态。
- 当前 transport 层已形成 `domain trait -> infra mock service -> app query -> command -> Vue service` 的只读链路，可在不改 UI 合同的前提下替换为真实网络实现。
- 当前 transport 层已补齐 `connect / disconnect / sync` 命令链路，Circle 详情页可直接触发 relay 状态变更，并同步更新 SQLite 聊天域与 transport snapshot。
- 当前 transport snapshot 已包含 `diagnostics / peers / sessionSync / activities` 四类数据，Circle 详情页可查看已发现 peer、会话同步状态和本地 runtime activity 时间线，并触发浏览器回退或 Tauri 桌面命令下的一致动作。
- 当前 transport 的 peer discovery 与 session sync 已有独立 SQLite cache 表，service 会在聊天域推导结果之上合并并回写缓存，为后续接真实网络适配器预留状态面。
- 当前 transport service 已不再把协议差异硬编码在单文件里，`websocket / mesh / invite` 已通过 adapter registry 分流，且已有基础单元测试覆盖关键分支。
- 当前 transport 已形成 `service orchestration -> engine -> protocol adapters` 的内部分层，mock 行为已从 service 中抽离，后续可直接增加真实 engine 实现并复用现有 SQLite/cache/snapshot 输出链路。
- 当前 transport engine 已具备统一选择入口，`experimental transport` 打开后会切到 `nativePreview` 占位 engine，Rust snapshot 与前端设置页都能看到当前 engine 标识。
- 当前 transport 状态构建逻辑已收敛到共享 builder，`mock / nativePreview` engine 基于同一套 seed/cache 推导流程独立产出 snapshot。
- 当前 transport 的 SQLite 编排入口已更名为 `LocalTransportService`，明确它是桌面本地服务层而不是远程后端。
- 当前 transport runtime activity 已持久化到 SQLite cache，并在浏览器 fallback 下复用上一轮 snapshot 历史，避免本地预览模式丢失最近 relay 动作轨迹。
- 当前 Circle 目录页已能直接看到每个 relay 的 transport 摘要和最近动作，设置高级页与 About 页也可查看 snapshot 活动摘要；在 diagnostics 或 experimental 模式下，前端会定时刷新 transport snapshot。
- 当前仓库前端包管理已统一为 `pnpm`，Tauri 前置构建命令、README、Agent 和项目文档都已切换到 `pnpm` 流程。
- 后续每完成一个里程碑，应同步更新本文件中的状态与清单。
