# Gap Checklist

更新时间: 2026-04-20

## 目标说明

本清单用于跟踪桌面重构版距离“功能级克隆”的差距，避免只看页面相似度而忽略真实行为闭环。

## P0

- [~] 真实消息收发闭环: 本地发送态、失败重试入口、单条状态回写、circle 恢复 `open` 后未签名消息的 `sending -> sent` 自动收敛、当前会话的新消息增量补拉、跨会话 `sessions / contacts / groups / circles` 壳数据回写、`SyncSessions` 触发的入站式消息合并、`remoteId / syncSource / ackedAt` 回执字段、独立 `merge_remote_session_messages` 合并入口，以及按 `remoteId` 合并投递回执的 `merge_remote_delivery_receipts` 入口都已接通；本地 runtime 自动确认现在也复用同一条 receipt merge 路径，并支持在 `remoteId` 尚未建立时按 `messageId` 回填确认结果；`SyncSessions` 的模拟同步结果也已从“builder 直写 seed”收敛为显式 transport chat effects 输出，再由 service 统一走共享 merge 路径；新的 `localCommand` runtime stdout 事件通道也已可把 JSON line 形式的远端消息/回执事件接进这条 merge 通道；仓库内也已补齐 companion `p2p-chat-runtime` 预览二进制、session-aware launch arguments、本地构建产物解析回退，以及 `Sync / DiscoverPeers / SyncSessions -> request-aware 本地 queue -> runtime stdout event` 这一条最小动作驱动闭环；preview runtime 还可直接把 peer presence、session sync 状态和 activity 写回 transport cache；本地 `nsec / hexKey` 文本消息现在还会在发送/重试时直接签出稳定的 Nostr kind-1 `eventId` 并写入 `remoteId`，完整 `signedNostrEvent` envelope 也会进入 SQLite/message DTO，heartbeat 也会自动把未派发的 signed outbound message 作为 `publishOutboundMessages` 入队；对于 websocket circle，preview runtime 也已会把这些 event 真实编码成 Nostr `EVENT` message 并写到 relay socket，再继续等待 relay `["OK", event_id, accepted, message]` 来回写 sent/failed，transport cache 也会持久化 outbound dispatch 记录做最小判重；但真实入站来源、真实远端同步、更完整的常驻 relay runtime、网络级失败重试和更完整的远端回执仍未打通。
- [~] 真实账号接入: 本地 onboarding 已改为多步骤向导，登录方式/非敏感凭据摘要/首个 circle 选择现在也已通过 `complete_login` 优先下沉到 Rust shell store，并在原生层做统一 access 解析与 `existing / invite / custom / restore` 统一解析；登出也已优先通过 `logout_chat_session` 回写原生 shell store；新的 `authRuntime` 合同也已把 `localProfile / pending / connected / failed` 这层状态稳定持久化到前后端 snapshot 与 About 页，其中本地 `nsec / hexKey / npub` 输入现在会在 Rust 里做真实校验，`nsec / hexKey` 还会派生 canonical `npub`，`npub` 会明确标成只读 `failed`，远端 `bunker / nostrconnect` 仍停留在 `pending`；`LoginAccessSummary / AuthRuntimeSummary` 也都会显式带上已验证 `pubkey`，About 页可直接显示该摘要；本地 `nsec / hexKey` 还会额外写入独立 native `auth_runtime_credential_store`，`AuthRuntimeSummary` 也显式带上 `credentialPersistedInNativeStore`，桌面端若发现 local-secret session 丢了这份 native credential，会自动把 runtime 改判成 `failed`；远端 `bunker / nostrconnect` 现在也要求匹配的 native `auth_runtime_binding_store` 仍然存在，否则 runtime 会自动降级 `failed`；同时 native `auth_access` 也开始真实解析 remote binding URI，校验连接 pubkey、relay 和 `nostrconnect` 的 `secret`，`AuthRuntimeBindingSummary` 也新增了 `connectionPubkey / relayCount / hasSecret / requestedPermissions / clientName` 这组诊断字段；新的 `sync_auth_runtime` 原生命令与前端 `pending` 轮询也已把“native store -> 当前壳 runtime”的同步边界固定下来，后续真实 signer worker 只需要写 `auth_runtime_state_store` 即可驱动 UI 收敛；旧 snapshot 在读取时也会自动回填这层状态；聊天发送入口现在也会真正按 `authRuntime` 禁用 `Send / Retry`，Rust `send_message / retry_message_delivery` 也会按 shell store 原生拒绝不可发送 runtime；本地 `nsec / hexKey` 文本消息在发送/重试时还会直接签出稳定 `eventId`，并把完整 `signedNostrEvent` envelope 持久化到消息记录，说明 native credential 已开始进入真实签名路径；新的 `update_auth_runtime` 命令与 About 诊断面板也已把 `pending -> connected/failed` 的原生写回边界固定下来；`authRuntime` 现在还新增了独立 native `auth_runtime_state_store`，`load / login / update / logout` 都会和它对齐，且 `AuthRuntimeSummary` 会显式带上 `persistedInNativeStore + credentialPersistedInNativeStore + canSendMessages + sendBlockedReason`，About 页和聊天主窗可直接复用这层诊断；远端 `bunker / nostrconnect` 的原始目标也会单独写进 native binding store 并在 shell 里保留脱敏摘要；但真实 signer 握手、signed event 发布、凭据管理和账号运行时还没有接通。
- [~] 消息模型扩展: 已补 `deliveryStatus + remoteId + syncSource + ackedAt` 基础合同与展示，也已补最小 `reply`、`mention`、`message detail`、file attachment send、image message 和 video message 交互；当前 file/image/video attachment 已从 SQLite 内联 `data:` 预览迁到原生 `chat-media/` 文件存储，消息 `meta` 现可持久化 `label + localPath`，并开始兼容 `remoteUrl` 这类后续真实媒体合同字段，timeline/detail 也可按本地 asset URL 或远端 URL 回放；Rust 侧 duplicate merge / relay echo 也已能按字段合并 `localPath + remoteUrl`，不会再互相覆盖；同时已新增最小 native cache/download 边界，可把 remote-only media 下载进 `chat-media/` 并回写 `localPath`；domain mutation 后也会做最小 orphan cleanup；但仍没有真实上传、更完整的引用计数/GC、reaction 或 file server；更完整的远端事件模型仍缺失。
- [~] 增量式消息存储: 常用聊天 mutation 已切到 SQLite 事务式增量写入，草稿字段、单条消息 `deliveryStatus` 以及 `remoteId / syncSource / ackedAt` 也已独立持久化；未签名本地消息的 `sending -> sent` 状态会随 circle 恢复自动补确认信息，而带 `signedNostrEvent` 的消息则会保持 `sending` 直到 runtime/relay receipt 回写，且这两条路径现在都收敛到共享 receipt merge/change-set 语义；聊天窗已切到 session 级分页补历史，当前会话能按最后一条消息做增量补拉，跨会话 `sessions / contacts / groups / circles` 也会回写壳数据，`SyncSessions` 也会把模拟入站消息写入 SQLite，新的远端合并入口也已支持按 `remoteId` 去重，远端回执也已可按 `remoteId` 或回退 `messageId` 独立落库；但真实远端发送态流转和回执状态回写仍未完成。

## P1

- [x] 多步骤登录与引导壳: 已补齐 `入口选择 -> 凭据 -> 资料 -> Circle` 的桌面 onboarding。
- [x] 动态用户资料持久化: `userProfile` 已进入统一 `ChatShellSnapshot`，前端与 Rust/SQLite 持久化合同一致。
- [x] 本地登录/登出壳语义: 登录表单现在会持久化非敏感 `authSession` 摘要并优先走 Rust shell store；登出也会优先清理原生 shell snapshot、同步本地 fallback，未认证启动不再把旧 circles 直接回灌到登录页。
- [~] Circle 生命周期: 已补删除后归档到本地 restore catalog、设置页按原 `relay / type / description` 恢复/遗忘，以及登录 `restore` 模式在本地归档项里显式选择要恢复的 circle；但购买恢复、远端权限校验和更真实的 Circle 准入规则仍缺失。
- [~] 新消息流程: 已补 `自聊 + 建群` 本地闭环，也补了 `new message -> select members -> group creation` 两步流；`Add Friends` 中若输入的是 invite/relay 风格文本，现也会优先走 circle join 分支，并在成功加入后回到新 circle 的建消息页；`Note to Self` 也已增加独立确认页；但二维码邀请和更完整的建群规则仍缺失。
- [~] 找人流程: 已补按 `handle / pubkey / invite-style text` 直接 lookup 拉起会话，也补了 `join-circle` 模式来处理 invite/relay 文本输入；聊天模式下识别到 circle 风格输入时，也不再错误创建 fake contact，而会默认转到 circle 导入；但二维码扫描和更真实的远端目录检索仍缺失。
- [~] 群和联系人管理: 已补群名编辑、成员总览、加人/移人二级页、联系人备注编辑，以及对应 Rust/SQLite 持久化链路；但联系人邀请、管理员操作和更多资料动作仍缺失。
- [~] 设置生效链路: `theme / textSize / html lang` 已开始真实驱动桌面壳外观与文档语言属性，但完整多语言文案、通知侧行为和更多细分页面的主题 token 仍未完全接通。

## P2

- [ ] 平台能力补齐: Android/iOS 的推送、分享扩展、语音/通话、权限链路尚未映射到桌面方案。
- [ ] 通话与媒体能力: 头部动作和消息类型还没有连到真实音视频或媒体处理能力。
- [~] 账号与 Circle 恢复文案: 已将设置入口改为 `Restore Circle Access`，并补齐真实恢复目录与恢复/遗忘动作；但恢复结果页、购买凭据恢复和远端权限确认仍未完成。

## 当前建议顺序

1. 先把当前已经落到原生 `chat-media/` 的 `file / image / video` 本地资产，继续收敛成真实 `upload / download / media URL` 合同，并把现在仅有的 orphan cleanup 推进到更完整的引用计数/GC 边界。
2. 再定义更完整的真实消息投递合同和远端回执模型，把现有本地增量刷新链路换成真实同步源。
3. 然后把 `existingAccount / signer` onboarding 继续推进到更完整的常驻 relay runtime / publish job / 重试退避链路，复用已经落好的 `sync_auth_runtime / auth_runtime_state_store / auth_runtime_binding_store / auth_access` 边界。

## 模块对照

- `ox_home`
  - 当前对位度较高
  - 已有: 主聊天壳、会话列表、归档、overlay/history、circle switcher、circle directory
  - 主要剩余: 更细的分享/转发流、部分列表交互细节、与真实数据面联动后的收尾校准
- `ox_login`
  - 当前对位度中等
  - 已有: 多步 onboarding、auth session/runtime、restore catalog、本地 secret / bunker 基础分流
  - 主要剩余: `capacity / duration / checkout / private cloud / activated / restore purchases` 这类私有 circle / 商业化流程
- `ox_chat`
  - 当前对位度中等
  - 已有: 文本聊天、自聊、建群、找人、联系人备注、群名/群成员管理、消息发送态、最小 reply、mention、message detail、最小 file/image/video message、最小 message action/report draft
  - 主要剩余: 真实 file/image/video upload/download、reaction、真实 moderation/report submit、更多真实远端事件语义
- `ox_chat_ui`
  - 当前对位度偏低
  - 已有: 自建 `ChatPane.vue`、基础时间线/发送区、图片/视频预览，以及最小 message action menu
  - 主要剩余: typing indicator、unread header、更多 chat list/config/theme 级细节，不应误判为“当前 UI 已基本等价”
- `ox_usercenter`
  - 当前对位度偏低到中等
  - 已有: 聚合型 `SettingsDetailPage.vue`，以及 `preferences / notifications / advanced / restore / about`
  - 主要剩余: `keys / QR / avatar / bio / nickname / file server / profile` 这批原项目页面和对应数据链路
- `ox_common`
  - 当前只映射了一部分
  - 已有: 本地壳状态、部分 theme/text size、生硬化的 scheme/clipboard/diagnostics 替代
  - 主要剩余: push、upload、scan、purchase、file server、tor、permission、share 等基础设施
- `ox_call`
  - 当前基本未开始
  - 主要剩余: call/webrtc 本身，以及聊天消息与 call session 的真实联动

## 当前判断

- 若只按“桌面壳 + 本地持久化 + preview transport”算，当前大约已到 `70%~80%`
- 若按“桌面端可用产品”算，当前大约在 `45%~55%`
- 若按“对 `tmp/xchat-app-main` 做功能级克隆”算，当前大约在 `35%~45%`
- 现阶段最重要的提醒不是“还有哪些页面没做”，而是:
  - 当前最大缺口已经不是壳层 UI，而是数据面与平台能力面
  - 在 P0 数据面没有收掉之前，不应长期停留在 `no-circle UX / warning dedupe / empty-state polish` 这类壳层打磨
  - 当前 transport 仍应被视作 `mock / nativePreview` 主导的预览链路，而不是“真实网络层已完成”
