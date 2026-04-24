# 当前版本状态

更新日期: `2026-04-23`  
当前应用版本: `0.1.0`

## 1. 文档目的

这份文档用于总结当前仓库版本的真实状态，重点回答 4 件事：

1. 当前 MVP 到了什么程度。
2. 哪些聊天主链能力已经真正接通。
3. 哪些能力仍然没有完成最终验收。
4. Android 构建链路目前处于什么状态。

本文优先记录已经通过代码、测试或脚本验证的事实，不把“已经写了代码”和“已经完成最终产品验收”混为一谈。

## 2. 一句话结论

当前仓库已经进入“桌面端简单 P2P/Nostr 聊天 MVP 基本可用”的阶段：

- Rust/Tauri 聊天主链已经打通到 `登录/认证状态 -> 本地签名或 runtime -> relay publish -> inbound merge -> receipt merge -> timeline/session 回写`。
- 仓库级自动化测试已经覆盖双账号消息往返、回执回写、reply preview、水合、去重与 transport service 路由组合点。
- Android APK 构建脚本已经补齐到“Debian/Ubuntu Linux 主机可按脚本准备依赖并执行构建”的程度。

但当前还不能把项目描述为“所有端都已完成最终验收”，因为：

- 还没有完成一次正式的桌面前台人工全链路验收。
- 还没有完成 Android 真机/模拟器端的聊天运行验收。
- 非 MVP 页面与非 MVP 业务并未完整复刻 `tmp/xchat-app-main`。

## 3. 当前产品范围

当前阶段产品范围已经明确收敛为：

- 基于公共 `wss` relay 的简单 P2P 聊天 MVP
- 重点支持桌面端聊天主链闭环
- UI 只要求在 **MVP 保留范围内** 尽量对齐 `tmp/xchat-app-main`

当前不作为主线目标的内容：

- 订阅、支付、private cloud、purchase、private relay backend
- 非 MVP 页面全量复刻
- Android 端完整聊天产品化验收

## 4. UI 状态

当前 UI 的实际状态是：

- 登录、onboarding、circle 入口、找人、会话与聊天主区域，已经按 `tmp/xchat-app-main` 的信息架构和 source-like 风格收拢过多轮。
- 当前目标不是“全量像素级复刻 Flutter 原项目”，而是“在 MVP 范围内尽量保持 source-like”。
- `tmp/xchat-app-main` 中非 MVP 的页面和流程，没有被重新拉回主流程。

目前仍需注意：

- 桌面 UI 虽然已经明显向参考项目靠拢，但还没有做一轮正式的人工视觉验收。
- Android 端 UI/运行态还没有做最终对照实跑，所以不能把“UI 已完全和参考项目一致”当成已完成结论。

## 5. 已完成的核心能力

### 5.1 桌面端基础壳

- 技术栈固定为 `Rust + Tauri 2 + Vue 3 + PrimeVue + TypeScript`
- 前后端边界已经基本稳定：
  - `src/` 负责界面与交互
  - `src-tauri/` 负责命令、存储、runtime、transport 与系统能力

### 5.2 认证与登录态

当前已经具备：

- 多步骤登录与 onboarding 壳
- `Get Started` 自动创建本地 Nostr 账号，并把私钥持久化到原生 credential store
- 可在设置页导出当前本地账号的 `nsec / hex` 私钥
- `nsec / hexKey / npub / bunker / nostrconnect` 的基础接入结构
- 本地 secret 凭据原生持久化
- shell auth state / native credential store / runtime summary 的原生侧持久化和回填
- 发送入口会根据 auth runtime 状态决定是否允许发送

当前未完成：

- 真实 remote signer 握手与完整生产级账号运行时
- Android 端账号链路验收

### 5.3 聊天与消息模型

当前已经具备：

- 文本消息发送
- reply preview
- `deliveryStatus / remoteId / syncSource / ackedAt`
- direct/group/self-chat 基础结构
- file/image/video 消息的基础模型与原生媒体存储
- receipt merge 和 duplicate merge 的共享语义

### 5.4 Relay / runtime / transport 主链

当前 Rust 侧已经具备：

- 本地 signed outbound message 生成与持久化
- outbound dispatch / inbound sync / receipt merge
- `Sync` / `SyncSessions` 的 transport chat effects 共享收敛路径
- local command runtime / preview runtime / runtime manager 的主链骨架
- 基于 relay 的 publish、回执、session 回写与基础 background sync

这意味着当前“代码层面”的主链已经不是假数据演示，而是实际具备 transport 和 receipt 路径的。

## 6. 本轮补强内容

这一轮新增或收敛了以下内容。

### 6.1 双账号功能闭环测试

在 [chat_mutations.rs](/home/sean/git/p2p-chat/src-tauri/src/app/chat_mutations.rs#L5121) 新增并跑通了：

- `two_local_accounts_can_exchange_direct_messages_and_reconcile_receipts`

这条测试验证了：

- A 账号发送消息
- B 账号合并收到的 relay 消息
- A 账号合并 delivery receipt
- B 账号基于收到的消息回复
- A 账号合并回复并水合 reply preview
- B 账号再合并 reply receipt

这条测试使用两套隔离的本地账号配置根，而不是单 store 假装双账号，因此可信度比普通单元测试更高。

### 6.2 回执幂等性测试

在 [chat_mutations.rs](/home/sean/git/p2p-chat/src-tauri/src/app/chat_mutations.rs#L5036) 新增并跑通了：

- `merge_remote_delivery_receipts_is_idempotent_for_duplicate_receipts`

这条测试锁住了：

- 同一批 receipt 重复输入不会让消息状态漂移
- 跨批次重复 receipt 不会重复写坏消息
- 消息 bucket 长度不会异常增长

### 6.3 Transport service 组合测试

在 [local_transport_service.rs](/home/sean/git/p2p-chat/src-tauri/src/infra/local_transport_service.rs#L3488) 新增并跑通了：

- `apply_transport_chat_effects_routes_peer_reply_and_reconciles_local_direct_receipt`

这条测试补上了之前缺的组合点：

- 同一批 `TransportChatEffects` 内
  - `remote_message_merges`
  - `remote_delivery_receipt_merges`
- 同时生效时，消息是否被正确路由到 direct session
- 本地待发送消息是否被正确置为 `Sent`
- reply preview 是否能被正确水合

这条测试的价值在于，它比单独的 mutation 测试更接近 transport glue 真实行为。

## 7. 当前验证状态

### 7.1 已通过的自动化验证

本轮完成后，以下结果已实测通过：

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

结果：

- Rust lib tests: `218 passed, 0 failed, 2 ignored`
- runtime bin tests: `21 passed, 0 failed, 1 ignored`

其中已经覆盖到：

- 双账号 direct message roundtrip
- receipt merge
- reply preview hydration
- relay echo / duplicate merge
- transport service 路由与回执组合逻辑

### 7.2 已验证的脚本

以下脚本验证已完成：

```bash
bash -n scripts/install-android-build-deps.sh
bash -n scripts/build-android-apk.sh
bash scripts/install-android-build-deps.sh --help
bash scripts/build-android-apk.sh --help
```

说明：

- 这些验证证明脚本语法与入口说明正常。
- 这不等于“已经在本轮完成实际 APK 构建产物验收”。

### 7.3 已有但仍属手动/ignored 的验证

仓库中仍保留 live relay smoke 测试能力，例如：

- runtime 级 public relay smoke
- service 层 `connect + publish + receipt merge + background sync` smoke
- service 层 auto-start publish smoke

这些能力说明真实 relay 路径不是完全空白，但它们当前仍不是默认常规测试的一部分。

## 8. Android 状态

### 8.1 已完成

当前 Android 构建链路已经有：

- 依赖安装脚本: [install-android-build-deps.sh](/home/sean/git/p2p-chat/scripts/install-android-build-deps.sh)
- APK/AAB 构建脚本: [build-android-apk.sh](/home/sean/git/p2p-chat/scripts/build-android-apk.sh)
- 本地 keystore 生成脚本: [generate-android-keystore.sh](/home/sean/git/p2p-chat/scripts/generate-android-keystore.sh)
- 一键 release APK 包装入口: [build_apk](/home/sean/git/p2p-chat/build_apk)
- Android 构建说明: [README.md](/home/sean/git/p2p-chat/README.md#L74)
- Android Tauri 配置: [tauri.android.conf.json](/home/sean/git/p2p-chat/src-tauri/tauri.android.conf.json)

脚本当前已补齐的关键点：

- Linux-only / Debian-Ubuntu-only 约束写清楚
- Node 版本与前置命令检查
- `pnpm` 激活与项目 JS 依赖安装
- `--target=aarch64,x86_64` 这类参数支持
- release / AAB 的签名配置检查
- 本地 `.local/android-upload.keystore` 生成与 `release/` 复制入口

### 8.2 当前结论

当前可以说：

- Android **构建链路准备工作** 已基本就位

当前不能说：

- Android **聊天产品本身** 已经完成
- Android **真机/模拟器运行** 已经通过验收

也就是说，现在的 Android 状态更接近：

- “可准备构建 APK”
- 而不是“Android 端已完成聊天交付”

## 9. 当前仍未完成的关键缺口

按优先级看，当前仍需谨慎对待的点主要有这些。

### 9.1 桌面前台人工验收还没正式收口

虽然 Rust/service/runtime 侧已经证明主链基本通了，但还缺一轮明确的前台人工验收，确认：

- 登录态是否稳定
- 选圈/建圈/进入会话是否顺滑
- 发消息后 timeline、receipt、transport notice 是否都正确收敛

### 9.2 Android 端没有完成运行验收

当前没有这一轮的：

- Android 模拟器验收
- Android 真机验收
- Android 端聊天链路验收

### 9.3 仍缺更强的真实在线端到端常规化测试

当前测试已经很强，但仍主要是仓库内 Rust 侧自动化：

- 没有浏览器 Playwright/Cypress 级 UI E2E
- 真实 relay live smoke 仍然是手动/ignored 流程

### 9.4 非 MVP 范围并未完成

当前并不是“完整复刻 xchat-app-main”状态。

当前更准确的表述是：

- `tmp/xchat-app-main` 的 **MVP 范围内页面和流程** 已明显收拢
- 非 MVP 模块仍然没有进入当前交付闭环

## 10. 适合对外/对内的当前表述

如果需要一句相对准确、不夸大的当前版本描述，建议这样写：

> 当前 `p2p-chat` 已完成桌面端简单 P2P/Nostr 聊天 MVP 的核心代码闭环，Rust/transport 主链与双账号消息往返、回执、reply preview 等关键功能已有自动化验证；Android APK 构建脚本已准备好，但 Android 端聊天运行和最终人工验收仍未完成。

## 11. 现在具体怎么用

这一节只写“当前最稳、最推荐”的使用方式。

### 11.1 桌面开发态运行

先安装依赖并启动桌面应用：

```bash
pnpm install
pnpm tauri:dev
```

如果只想先确认编译和测试是否正常，可以先跑：

```bash
pnpm check
pnpm native:test
```

### 11.2 当前最推荐的登录方式

当前最推荐、最稳的路径是：

- 直接点 `Get Started` 创建一个新账号
- 然后到 `Settings -> About -> Private Key Export` 备份 `nsec` 或 `hex`
- 使用 `nsec` 或 `64 位 hex 私钥` 登录（`0x` 前缀可选）
- 使用公共 `wss` relay
- 不把 `Get Private Circle` 当成当前主链

虽然 UI 和底层也保留了 `bunker://` / `nostrconnect://` 的结构与兼容，但如果你的目标是“先稳定发消息”，当前仍优先建议用本地 secret 路径，也就是 `Get Started` 或直接导入 `nsec/hex`。

### 11.3 登录步骤

打开桌面应用后，按这个顺序走：

1. 如果你要创建新账号，先点 `Get Started`。
2. 输入昵称并完成 profile 初始化。
3. 进入 circle 选择页后，优先选 `Custom Relay`。
4. 在弹出的 `Add Custom Relay` 里输入：
   - 完整 relay URL，例如 `wss://nos.lol`
   - 或 shortcut，例如 `damus`，界面会自动展开成对应的 `wss://...`
5. 完成登录后，进入 `Settings -> About -> Private Key Export`，先备份：
   - `nsec`
   - 或 `64 位 hex` 私钥（`0x` 前缀可选）

如果你已经有现成账号，则走这条路径：

1. 在登录页选择 `I have a Nostr account`。
2. 在输入框里填：
   - `nsec1...`
   - 或 64 位 hex 私钥（`0x` 前缀可选）
3. 继续进入 circle 选择页。
4. 优先选 `Custom Relay`，填完整 relay URL，例如 `wss://nos.lol`。
5. 点 `Join` 或继续完成连接流程。

更稳的建议：

- 如果是第一次用，优先直接填完整 `wss://...` 地址。
- `Get Private Circle` 当前不是推荐主链，它更像预留/预览入口，不适合拿来验证 MVP 聊天闭环。

### 11.4 进入聊天后怎么发消息

登录并进入主界面后，当前推荐这样用：

1. 先进入 `New Message` 或找人页。
2. 通过 `handle / pubkey / invite-style text` 拉起一个联系人会话。
3. 进入会话后直接发送文本消息。
4. 观察消息状态是否从 `Sending` 收敛。

当前已经具备并值得重点关注的现象：

- 发送后的本地消息会生成稳定的 `remoteId`
- receipt 回来后消息会从 `Sending` 收敛到 `Sent`
- reply 消息会带 reply preview
- 对端消息 merge 后会更新 session subtitle 和 unread

### 11.5 如果你要验证“两账号互发”

当前代码侧已经自动化验证过双账号互发，但如果你要手工验证，建议这么做：

1. 准备两个不同的 `nsec` 或 hex 私钥（hex 可带 `0x` 前缀）。
2. 让两个账号都连接到同一个公共 relay。
3. 在两个独立运行的应用实例中分别登录。
4. 用 A 给 B 发一条文本。
5. 确认 B 端收到后再回一条。
6. 观察双方消息是否正确进入各自 session，且发送态是否收敛。

注意：

- 仓库里已经做过“双账号消息往返 + 双向 receipt”自动化测试。
- 但桌面前台人工双开验收，当前仍建议你自己再做一轮。

### 11.6 当前不建议怎么用

下面这些路径现在不是“最推荐验证方式”：

- 把 `Get Private Circle` 当成当前主聊天主链
- 把 Android 端直接当成“已验收完成的聊天客户端”
- 把 remote signer 路径当成第一优先验证路径

不是说这些代码完全没有，而是它们当前不如“本地 secret + 公共 relay + 桌面端”这条主链稳定。

### 11.7 Android APK 现在怎么用

如果你的目标是先把 APK 构建出来，按这个流程走。

准备依赖：

```bash
pnpm android:deps
pnpm install --frozen-lockfile
```

导出环境变量：

```bash
export ANDROID_HOME="$HOME/Android/Sdk"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export PATH="$ANDROID_SDK_ROOT/platform-tools:$ANDROID_SDK_ROOT/cmdline-tools/latest/bin:$PATH"
source "$HOME/.cargo/env"
```

如果你想直接走本地 keystore + 一键 release APK：

```bash
pnpm android:release
```

它会在首次缺少 keystore 时自动创建 `.local/android-upload.keystore`，并把签名后的 APK 复制到 `release/`。

构建默认 APK：

```bash
pnpm android:apk
```

构建多个 ABI：

```bash
pnpm android:apk -- --target aarch64 armv7 x86_64
```

构建 split-per-abi：

```bash
pnpm android:apk -- --target=aarch64,x86_64 --split-per-abi
```

构建 AAB：

```bash
pnpm android:apk -- --aab --target aarch64
```

当前 Android 使用结论要分开看：

- 可以用这些脚本准备依赖并尝试产出 APK/AAB
- 但不要把这等同于“Android 聊天功能已经完成最终验收”

## 12. 下一步建议

如果继续推进，优先级建议如下：

1. 做一轮桌面前台人工验收，正式确认“代码主链已通”到“产品主链已通”。
2. 在 Linux 环境实际跑一次 `pnpm android:apk`，把 APK 构建链路从“脚本可用”推进到“产物可出”。
3. 做 Android 模拟器或真机最小验收，确认至少能启动、登录、进入聊天壳。
4. 继续把真实 relay live smoke 收敛为更稳定的常规化验证。
