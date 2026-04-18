# Gap Checklist

更新时间: 2026-04-18

## 目标说明

本清单用于跟踪桌面重构版距离“功能级克隆”的差距，避免只看页面相似度而忽略真实行为闭环。

## P0

- [~] 真实消息收发闭环: 本地发送态、失败重试入口、单条状态回写、circle 恢复 `open` 后的 `sending -> sent` 自动收敛、当前会话的新消息增量补拉、跨会话 `sessions / contacts / groups / circles` 壳数据回写、`SyncSessions` 触发的入站式消息合并、`remoteId / syncSource / ackedAt` 回执字段、独立 `merge_remote_session_messages` 合并入口，以及按 `remoteId` 合并投递回执的 `merge_remote_delivery_receipts` 入口都已接通；本地 runtime 自动确认现在也复用同一条 receipt merge 路径，并支持在 `remoteId` 尚未建立时按 `messageId` 回填确认结果；`SyncSessions` 的模拟同步结果也已从“builder 直写 seed”收敛为显式 transport chat effects 输出，再由 service 统一走共享 merge 路径；新的 `localCommand` runtime stdout 事件通道也已可把 JSON line 形式的远端消息/回执事件接进这条 merge 通道；仓库内也已补齐 companion `p2p-chat-runtime` 预览二进制、session-aware launch arguments、本地构建产物解析回退，以及 `Sync / DiscoverPeers / SyncSessions -> request-aware 本地 queue -> runtime stdout event` 这一条最小动作驱动闭环；preview runtime 还可直接把 peer presence、session sync 状态和 activity 写回 transport cache；但真实入站来源、真实远端同步、网络级失败重试和远端回执仍未打通。
- [ ] 真实账号接入: 本地 onboarding 已改为多步骤向导，且登录方式/非敏感凭据摘要/restore 选择已进入本地持久化合同；但 `nsec / signer / 导入账号` 还没有接入真实运行时。
- [~] 消息模型扩展: 已补 `deliveryStatus + remoteId + syncSource + ackedAt` 基础合同与展示，但回复、提及、图片、视频、反应和更完整的远端事件模型仍缺失。
- [~] 增量式消息存储: 常用聊天 mutation 已切到 SQLite 事务式增量写入，草稿字段、单条消息 `deliveryStatus` 以及 `remoteId / syncSource / ackedAt` 也已独立持久化，本地 `sending -> sent` 状态会随 circle 恢复自动补确认信息，且这条路径现在与远端 receipt 共用同一套 merge/change-set 语义；聊天窗已切到 session 级分页补历史，当前会话能按最后一条消息做增量补拉，跨会话 `sessions / contacts / groups / circles` 也会回写壳数据，`SyncSessions` 也会把模拟入站消息写入 SQLite，新的远端合并入口也已支持按 `remoteId` 去重，远端回执也已可按 `remoteId` 或回退 `messageId` 独立落库；但真实远端发送态流转和回执状态回写仍未完成。

## P1

- [x] 多步骤登录与引导壳: 已补齐 `入口选择 -> 凭据 -> 资料 -> Circle` 的桌面 onboarding。
- [x] 动态用户资料持久化: `userProfile` 已进入统一 `ChatShellSnapshot`，前端与 Rust/SQLite 持久化合同一致。
- [x] 本地登录/登出壳语义: 登录表单现在会持久化非敏感 `authSession` 摘要；登出会清空 shell/domain 预览，未认证启动也不会再把旧 circles 直接回灌到登录页。
- [ ] Circle 生命周期: 恢复、加入、切换、购买恢复仍是本地壳逻辑，需要补齐真实规则和结果状态。
- [~] 新消息流程: 已补 `自聊 + 建群` 本地闭环，也补了 `new message -> select members -> group creation` 两步流；但邀请导入后的会话创建分支、更接近原项目的自聊确认/二维码邀请，以及更完整的建群规则仍缺失。
- [~] 找人流程: 已补按 `handle / pubkey / invite-style text` 直接 lookup 拉起会话，也补了 `join-circle` 模式来处理 invite/relay 文本输入；但二维码扫描和更真实的远端目录检索仍缺失。
- [~] 群和联系人管理: 已补群名编辑、成员总览、加人/移人二级页和对应 Rust/SQLite 持久化链路；但联系人备注、邀请、管理员操作和更多资料动作仍缺失。
- [~] 设置生效链路: `theme / textSize / html lang` 已开始真实驱动桌面壳外观与文档语言属性，但完整多语言文案、通知侧行为和更多细分页面的主题 token 仍未完全接通。

## P2

- [ ] 平台能力补齐: Android/iOS 的推送、分享扩展、语音/通话、权限链路尚未映射到桌面方案。
- [ ] 通话与媒体能力: 头部动作和消息类型还没有连到真实音视频或媒体处理能力。
- [ ] 账号与 Circle 恢复文案: 已将设置入口改为 `Restore Circle Access`，但恢复结果页和恢复动作还需要真实化。

## 当前建议顺序

1. 先定义真实消息投递合同和远端回执模型，再把现有本地增量刷新链路换成真实同步源。
2. 再把 `existingAccount / signer` onboarding 接到真实本地 runtime。
3. 然后补新消息、找人、群资料这些高频交互缺口。
