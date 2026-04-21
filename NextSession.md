# Next Session

更新时间: 2026-04-21

## 当前硬约束

- 当前唯一 P0 目标: `通过 Nostr relay 完成真实聊天`。
- 所有需要后端/对象存储/圈子服务端/支付订阅服务配合的任务，全部暂停。
- 暂停范围明确包括:
  - `file server` 管理页与模型迁移
  - `NIP-96 / Blossom / MinIO / S3 / presigned`
  - `upload / download / media backend`
  - `purchase / subscription / private cloud / paid relay backend`
- 这是一个重构项目；对标对象仍是 `tmp/xchat-app-main`，但当前阶段只对齐 `Nostr 文本聊天主链`，不继续追后端依赖功能。
- UI 重构也必须服从“源项目对齐”原则，尤其登录/引导页先对齐 `tmp/xchat-app-main/packages/business_modules/ox_login`，不要继续做偏离原项目结构的自定义桌面营销页。
- 除非用户明确要求，否则下个 session 不要再做不可控 GUI 自动点按/自动截图式验收；优先源码比对、静态实现、构建检查。

## 当前已确认状态

- 原项目不是“完全没有后端”，也不是“本仓库现在必须先补后端”。
- 现在已经明确冻结原项目里那些依赖服务端的部分，当前只做无需后端的 Nostr 主链。
- 当前仓库里已经具备这些 Nostr 基础能力:
  - 本地 `nsec / hexKey` 登录、校验、真实 text-note 签名
  - `bunker / NIP-46` 握手与远端 `sign_event`
  - relay `EVENT` 发布与 `OK true/false` 回执处理
  - inbound relay kind-1 同步、SQLite merge、session reroute、去重
  - direct/group/self-authored relay message 的基础路由

## 本轮刚完成

- [LoginScreen.vue](/home/sean/git/p2p-chat/src/components/LoginScreen.vue) 已做第一轮源项目回拢：
  - 去掉了之前那套“双栏大白卡 + 4 步 marketing flow”的自定义入口
  - 改成更接近原 Flutter 登录页的“全屏渐变 + 单列 onboarding 轮播 + 底部两个主入口按钮”
  - 详细输入被拆回后续页：`account access`、`profile`、`circle`
  - 轮播素材直接复用了 `tmp/xchat-app-main/packages/business_modules/ox_login/assets/images/material_onboarding-*.png`
- 登录流又做了一轮结构收敛：
  - `quick start` 现在仍走 `profile -> circle`
  - `existing account` 现在改成更接近原项目：
    - 先走单字段 `account key` 登录页
    - 若当前壳里已经有 saved circles，则直接登录回圈子，不再强制多走一页 `circle setup`
    - 只有确实没有可用圈子时，才继续进入 `circle` 选择
- 又做了一轮更激进的视觉回拢，当前 [LoginScreen.vue](/home/sean/git/p2p-chat/src/components/LoginScreen.vue) 现在更接近 Flutter 源页骨架：
  - 登录入口页改成窄内容区、移动端 onboarding 风格，不再是桌面白卡分栏
  - `account key` 页改成顶部 `LOGIN` 栏 + 左对齐标题 + 底部固定主按钮
  - `profile` 页改成更像原项目的头像预览 + 两个名字输入，不再暴露一堆桌面额外字段
  - `circle` 页改成列表型 option card：`I have an invite` / `Get Private Circle` / `Custom Relay`
  - `Private Circle` 卡片视觉上已补回，但仍保持禁用，因为当前阶段后端相关能力继续暂停
- 又继续做了一轮视觉细节收紧：
  - 去掉了不属于源项目登录页的 `summary card`
  - 去掉了不属于源项目 circle 页主结构的 `saved circle` 入口卡片
  - 把 circle 页主容器从“浮层卡片”继续收成更像移动端白底窄栏
  - terms 文案改回更像源项目的下划线 link 样式
  - 修掉了一个实际逻辑缺口：`custom relay` 之前只填 relay 也会被隐藏的 `customCircleName` 校验卡住，现在已收敛
- 登录 access 页也已按原项目 `account_key_login_page.dart` 收敛成单字段入口：
  - 同一输入框接受 `nsec / npub / raw hex / bunker:// / nostrconnect://`
  - 并明确提示当前真正可发消息的可靠路径仍是 `nsec / raw hex`
- 登录流又补了一轮静态收敛和排查结论：
  - [LoginScreen.vue](/home/sean/git/p2p-chat/src/components/LoginScreen.vue) 在 `onMounted` 时现在会显式 reset 到入口态：
    - `currentStep = 0`
    - `selectedMethod = quickStart`
    - 清空 `accountKey / invite / custom relay`
    - 重新按当前 `profile / restorable circles` 生成初始表单值
  - 这意味着从源码上看，`LoginScreen.vue` 自己已不再存在“干净挂载直接跳到 Profile step”的路径
  - circle selection 页也进一步收紧到更接近源项目：
    - 不再默认落到一个 UI 上没有入口的 `existing` mode
    - `quick start` 现在固定从 `invite/custom` 结构进入 circle 选择
    - 第三步底部主按钮文案改成更接近源项目的 `Connect`
  - account key 前端校验也收紧：
    - `hex` 现在按 `64` 位校验，不再是之前的宽松 `32+`
    - `nsec / npub` 不再只看前缀
    - `bunker:// / nostrconnect://` 现在会校验 pubkey + relay 参数；无效 URI 会有 inline error
  - existing account 单字段页在提交时会按 access kind 映射真实 login method：
    - 本地 key 仍走 `existingAccount`
    - `bunker:// / nostrconnect://` 会转成 `signer` payload，和 Rust 侧 auth model 对齐
- circle selection 页又继续往源项目收了一轮：
  - 主页面现在更接近源项目的“只展示选项卡 + restore link + 底部 Connect”
  - `invite / custom relay` 的具体输入不再直接摊在主页面，而是收进一个独立 sheet
  - `custom relay` sheet 补了更接近源项目 hint 的快捷项：`0xchat` / `damus`
  - 前端 custom relay 校验也改成和 Rust 侧 `relay_looks_valid(...)` 对齐：
    - 裸 relay 名称会先按 `wss://...` 预归一化再判定
    - 所以 `damus` 这类输入现在不会再被前端误拦
- 登录引导页又补回一轮源项目说明层：
  - account key 页的 `Learn more` 现在会弹出 `Understanding Nostr` info sheet
  - custom relay sheet 的 `What is a Relay?` 现在会弹出 `What is a Circle?` info sheet
  - 自定义 relay sheet 也补回了更接近原项目的 URL/shortcut hint，不再只剩 placeholder
- 这轮没有做 Android 真机或模拟器实测：
  - 当前对 Android/Flutter 版本的判断，来自 `tmp/xchat-app-main` 源码、资源、文案和页面结构比对
  - 不是 Android runtime 的端到端验收
- 做过一次干净配置下的桌面预览尝试，但 GUI 自动化不可控：
  - 预期应先看到登录入口页
  - 实际抓图却直接落在 `Profile` step
  - 目前静态排查第一轮结论是：
    - `LoginScreen.vue` 自己的 mount 初始 step 逻辑已显式锁回入口态
    - logged-out 状态下 overlay hash 也会被 sanitize，不会保留前台 overlay 去冒充登录步骤
    - 若后面仍复现，更像 GUI 自动化输入噪声 / 焦点误点 / 抓图误判，而不是当前登录页源码仍在自动跳 step
  - 按用户要求，先停止 GUI 自动测试，不再用它作为结论依据
- `SyncSessions` 现在和 `Sync` 共用同一套 relay `since + filters` 生成逻辑，不再只是清 unread 或带出站消息。
- `p2p-chat-runtime` 现在会对 `SyncSessions` 也执行真实 relay bounded read:
  - `REQ -> EVENT/EOSE -> CLOSE`
  - 所以前端点 `Sync Sessions` 时也会真实拉入站消息。
- `load_snapshot` hydrate 路径现在会给 live runtime 追加静默 background relay sync:
  - 只做 inbound relay merge
  - 不清 unread
  - 不追加普通 `Sync` activity
  - 同 circle 走持久化 5 秒节流
- relay sync filter 现在已补上“当前账号自己发出的事件”:
  - direct: `authors=[currentUser] + #p=[direct contacts]`
  - group: `authors=[currentUser] + #p=[group members]`
  - self-chat: `authors=[currentUser]`
- 这意味着“我在别的设备发出的 direct/group/self-chat kind-1 事件”现在也能被当前桌面端拉回，再走现有 session reroute。
- 前端 `useChatShell` 现在不再把 transport heartbeat 绑定到 `relayDiagnostics / experimentalTransport` 开关:
  - 只要已登录且当前存在 `ws/wss` relay，就会默认周期性刷新 transport snapshot
  - 默认 heartbeat 为 `8s`
  - runtime recovery/backoff 时自动收紧到 `3s`
  - diagnostics 打开时维持 `12s`
- 这让 Rust 侧的 hydrate background relay sync 在普通 Nostr 聊天场景下也能持续被触发，而不是只在手动刷新或诊断模式下生效。
- `p2p-chat-runtime` 的 outbound relay publish warn 现在已从单一 generic 失败，收紧成结构化分类:
  - `reject -> Relay rejected event`
  - `close -> Relay closed publish connection`
  - `timeout -> Relay publish timed out`
  - 其他保底 -> `Relay publish failed`
- runtime warn detail 里的失败计数现在按真实 failed receipt 计，不再直接拿整批 outbound 数量。
- 前端 `useChatShell` 现在会把上述 publish 类 runtime warn 映射成 transport notice，用户不必进 diagnostics 面板才能看到这类发送失败。
- 前端 transport recovery 现在也会作用在真实 Tauri snapshot，不再只在 fallback 模式下做本地假恢复。
- `deriveRuntimeRetryAction(...)` 现在会先识别“desired running 但还没有任何 launch 痕迹”的 local-command runtime，并优先触发 `connect`，而不是错误地先发 `sync`。
- 这修掉了一个关键主链缺口:
  - 登录后/首次 snapshot 后，runtime 不再需要用户手动先点一次 `Connect` 才有机会真正拉起
  - 后续 heartbeat 可以继续自动推进 `connect -> sync -> inbound relay merge`
- Rust 侧 `local_transport_recovery_worker` 现在也和前端 recovery 语义对齐：
  - 对“desired running，但当前还没有任何 launch 痕迹”的 local-command runtime，首次恢复动作直接走 `Connect`
  - 不再先走一轮假 `Sync`，再因为无 managed handle 掉进 `3s` backoff
  - 这让“新登录 / 新建圈 / 首次 hydrate”更接近真正的无手动 `Connect` 主链
- 前后端默认配置现在都改成 `experimentalTransport = true`:
  - 新默认不会再优先落到 mock transport
  - 登录完成时也会强制把原生 shell 的 `experimental_transport` 拉到 `true`
  - 旧的已认证壳状态在启动加载时也会自动迁移到 `true`
- 自定义 circle 的 relay 输入现在会做 Nostr websocket 归一化:
  - 输入 `relay.example.com` 会自动变成 `wss://relay.example.com`
  - 带非 `ws/wss` scheme 的地址会被拦下
  - 这样不会再出现 UI 看似建圈成功，但 runtime 侧 `Url::parse` 失败导致实际不可连的情况
- `p2p-chat-runtime` 的 relay 建连现在补了显式 `5s` connect timeout：
  - 不再因为坏 relay / 黑洞地址在 `TcpStream::connect` 阶段长时间卡死
  - direct 直连时会优先尝试 IPv4，再尝试 IPv6
- `p2p-chat-runtime` 现在还会尊重当前环境里的 `https_proxy / http_proxy`（HTTP CONNECT 隧道）和 `NO_PROXY`：
  - 这修掉了一个真实环境缺口: 在需要代理出站 443 的网络里，runtime 之前根本打不到公开 relay
  - 本地 `127.0.0.1 / localhost` 测试流仍会按 `NO_PROXY` 直连，不会被代理污染
- 已做真实公开 relay smoke：
  - 命令: `P2P_CHAT_LIVE_RELAY_URLS='wss://nos.lol,wss://relay.primal.net,wss://relay.snort.social,wss://relay.damus.io' cargo test --manifest-path src-tauri/Cargo.toml --bin p2p-chat-runtime manual_live_public_relay_smoke -- --ignored --nocapture`
  - 结果: `wss://nos.lol` 成功完成 `publish + sync read-back`
  - 本次最新实测成功 event id: `76c7682b79526ebd166f8d3f2e2adfbf403a15ee75fea7e9e73b2e383e9cafe9`
  - 这次验证的是 runtime 真正使用的 websocket publish / bounded sync 代码路径，不是临时单独写的外部脚本
- 已补一条更高一层的 ignored service smoke：
  - 测试名: `manual_live_public_relay_connect_publish_smoke`
  - 位置: [local_transport_service.rs](/home/sean/git/p2p-chat/src-tauri/src/infra/local_transport_service.rs)
  - 行为: 显式 `Connect` -> 轮询 `load_snapshot()` -> 检查 outbound dispatch / background sync marker / 本地消息 delivery receipt
- 这条 service smoke 现在也已通过：
  - 说明 Rust service 层已经验证到 `connect -> managed runtime -> queue publish -> receipt merge -> local message Sent -> background relay sync marker`
  - 这比单纯 runtime bin smoke 更接近真实桌面链路
- 又补了一条更贴近前台壳语义的 ignored service smoke：
  - 测试名: `manual_live_public_relay_snapshot_autostart_publish_smoke`
  - 行为: 不显式 `Connect`，只做 `send_message -> load_snapshot()`，验证 recovery/auto-start 会自己拉起 runtime，再完成 publish + receipt merge + background sync marker
  - 这条测试现在也已通过
  - 说明“无手动 Connect 的 auto-start publish 主链”在 Rust service 层和公开 relay 上也已经实证通过
- 额外排查结论：
  - 直接在 shell 里单独启动 `src-tauri/target/debug/p2p-chat-runtime preview-relay --relay-url wss://nos.lol --circle main-circle --session mika`，进程会持续存活
  - 再手工向 queue 文件写 `publishOutboundMessages`，runtime 会正常输出 `delivery receipt + warn activity`
  - 根因已经确认并修掉：
    - 之前会把“缓存里标成 live 的 local-command runtime”继续当成活进程
    - 但当前 app session 里其实没有对应的 managed process handle
    - 结果就是 UI/服务层会继续把 sync/publish 入队给一个并不存在的 companion runtime，表现成“snapshot 看起来 Active，但消息一直 Sending”
  - 修复后：
    - `probe_local_command_runtime(...)` 会把“当前 session 无 managed handle”视为不可用，而不是继续当成 live
    - `Connect` 现在遇到这种伪活跃 runtime 会真正重拉 companion runtime
    - `Sync / SyncSessions / background publish / background relay sync` 也不再对这种伪活跃 runtime 盲目入队

## 仍未完成的 P0 缺口

1. 公开 relay 的 runtime 级、Rust service 显式 `Connect` live smoke，以及 Rust service 无手动 `Connect` auto-start live smoke 现在都已实证通过，但“桌面 UI 登录态 -> 自动拉起 runtime -> 建圈/选圈 -> 发消息 -> heartbeat/hydrate 自动回流到正确 session”这条完整前台链路还没做人工端到端验收，所以暂时还不能把“已经完全可聊”当成最终验收完成。
   - 不过当前代码已经进一步收紧到“首次 bootstrap connect 不再额外等待一轮 backoff”，并且“无手动 Connect 的 auto-start publish”也已经做过 live smoke，所以离真实前台可聊只剩桌面 UI 验收，不再是 transport 主链明显断链。
   - 同时，登录 UI 虽已按源项目做了第一轮回拢，但还没有经过用户主导的视觉验收，也没有 Android 端对照实跑。
2. inbound sync 仍不是常驻 relay subscription；真实收消息延迟仍取决于 snapshot refresh 周期。
3. `publish -> relay OK/failed -> timeline` 这条状态已补上 runtime warn 分类，但还可以继续收紧到更前台/更贴消息的用户可见错误表面。
4. direct/group/self-chat 的 self-authored reroute 已有基础实现，但还需要继续用真实 relay 数据验证:
   - 同 relay 多圈
   - 缺 tag 或弱 tag
   - 旧 event replay / duplicate echo
5. pasted `nostrconnect://` 仍未支持本地生成 client URI 的完整授权流，当前继续视为不可发送。

## 下个 Session 直接做

1. 先不要再开 GUI 自动化；先做静态核对和代码收敛:
   - 对照 `tmp/xchat-app-main/packages/business_modules/ox_login/lib/page/login_page.dart`
   - 对照 `tmp/xchat-app-main/packages/business_modules/ox_login/lib/page/account_key_login_page.dart`
   - 对照 `tmp/xchat-app-main/packages/business_modules/ox_login/lib/page/profile_setup_page.dart`
   - 对照 `tmp/xchat-app-main/packages/business_modules/ox_login/lib/page/circle_selection_page.dart`
   - 把当前 [LoginScreen.vue](/home/sean/git/p2p-chat/src/components/LoginScreen.vue) 剩余不一致处继续收紧
   - 重点继续看 `circle selection` 页和原项目是否还存在结构偏差
2. “干净配置直接落到 Profile step”这件事，当前静态排查先收口为：
   - `LoginScreen.vue` 自身的初始 step 已显式 reset，先不再把主要怀疑点放在这里
   - 若后续还复现，优先看人工操作下是否真能稳定复现，而不是继续看 GUI 自动化抓图
   - 真要继续追，就看更上层的启动时序：
     - `App.vue` 登录态切换
     - launch overlay 退场时机
     - 是否存在外部自动输入/回车/焦点噪声
3. 在用户允许的前提下，再做可控的桌面人工验收，把“runtime 已通”推进到“桌面壳主链已通”:
   - 用当前桌面壳登录一个本地 secret 账号
   - 建一个 `wss://nos.lol` custom circle，确认不手动点 `Connect` 也能自动拉起 runtime
   - 直接在 UI 发一条文本消息，确认 receipt、timeline、transport notice 都收敛正常
   - 登录后不点按钮，只靠 heartbeat + hydrate，确认新入站消息能自动进入正确 session
   - 输入不带 scheme 的 relay 域名，确认建圈后实际保存为 `wss://...` 并可连
4. 用真实 relay 数据继续验证 [p2p-chat-runtime.rs](/home/sean/git/p2p-chat/src-tauri/src/bin/p2p-chat-runtime.rs) + [local_transport_service.rs](/home/sean/git/p2p-chat/src-tauri/src/infra/local_transport_service.rs) 的 reroute 边界:
   - 同 relay 多圈
   - 缺 tag / 弱 tag
   - replay / duplicate echo
   - 跨设备 self-authored direct/group/self-chat 回流
5. 继续把 relay receipt/error surface 往 UI 收:
   - failed message retry 后的 timeline 收敛
   - 是否需要把 explicit reject/timeout/close 进一步贴到消息级状态
   - 是否需要把 sync read fail 之类非 publish warn 也收进更克制的 notice 策略
6. 若 heartbeat 实测仍不够，再决定是否上更短周期或更明确的前台/后台 refresh 策略；暂时不要扩成后端或新服务。

## 关键文件

- [src-tauri/src/bin/p2p-chat-runtime.rs](/home/sean/git/p2p-chat/src-tauri/src/bin/p2p-chat-runtime.rs)
- [src-tauri/src/infra/local_transport_service.rs](/home/sean/git/p2p-chat/src-tauri/src/infra/local_transport_service.rs)
- [src-tauri/src/infra/local_transport_runtime_manager.rs](/home/sean/git/p2p-chat/src-tauri/src/infra/local_transport_runtime_manager.rs)
- [src-tauri/src/app/shell_auth.rs](/home/sean/git/p2p-chat/src-tauri/src/app/shell_auth.rs)
- [src-tauri/src/app/chat_mutations.rs](/home/sean/git/p2p-chat/src-tauri/src/app/chat_mutations.rs)

## 验证状态

- `cargo fmt --manifest-path src-tauri/Cargo.toml` 已执行
- `cargo test --manifest-path src-tauri/Cargo.toml` 通过
- `pnpm check` 通过
- `pnpm build` 通过
- `P2P_CHAT_LIVE_RELAY_URLS='wss://nos.lol,wss://relay.primal.net,wss://relay.snort.social,wss://relay.damus.io' cargo test --manifest-path src-tauri/Cargo.toml --bin p2p-chat-runtime manual_live_public_relay_smoke -- --ignored --nocapture` 通过
  - 公开 relay 实测成功: `wss://nos.lol`
  - 成功 event id: `76c7682b79526ebd166f8d3f2e2adfbf403a15ee75fea7e9e73b2e383e9cafe9`
- `cargo test --manifest-path src-tauri/Cargo.toml manual_live_public_relay_connect_publish_smoke -- --ignored --nocapture`
  - 现在通过
  - service 层 live smoke 已验证 `connect + publish + receipt merge + background sync marker`
- `cargo test --manifest-path src-tauri/Cargo.toml manual_live_public_relay_snapshot_autostart_publish_smoke -- --ignored --nocapture`
  - 现在通过
  - service 层 live smoke 已验证“不手动 Connect，只靠 snapshot/recovery 自动拉起 runtime”也能完成 `publish + receipt merge + background sync marker`
- `git diff --check` 通过
- 当前通过计数:
  - Rust lib tests: `199` 常规 + `2` ignored live smoke 手动通过
  - runtime bin tests: `21` 常规 + `1` ignored live smoke 手动通过

## 工作区备注

- worktree 本来就是 dirty 的，不要回退无关改动。
- [src-tauri/src/infra/media_upload.rs](/home/sean/git/p2p-chat/src-tauri/src/infra/media_upload.rs) 仍是未跟踪文件，但当前阶段不要再继续沿这个方向扩展。
