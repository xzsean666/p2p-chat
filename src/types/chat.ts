export type SessionKind = "direct" | "group" | "self";
export type MessageKind = "text" | "image" | "video" | "file" | "audio" | "system";
export type ChatMediaKind = "file" | "image" | "video";
export type MessageAuthor = "me" | "peer" | "system";
export type MessageDeliveryStatus = "sending" | "sent" | "failed";
export type MessageSyncSource = "local" | "relay" | "system";
export type CircleType = "default" | "paid" | "bitchat" | "custom";
export type CircleStatus = "open" | "connecting" | "closed";
export type SessionAction = "pin" | "mute" | "archive" | "delete" | "unarchive";
export type CircleCreateMode = "invite" | "private" | "custom";
export type LoginMethod = "quickStart" | "existingAccount" | "signer";
export type LoginCircleSelectionMode = "existing" | "invite" | "custom" | "restore";
export type LoginAccessKind =
  | "localProfile"
  | "nsec"
  | "npub"
  | "hexKey"
  | "bunker"
  | "nostrConnect";
export type AuthRuntimeState = "localProfile" | "pending" | "connected" | "failed";
export type TransportHealth = "online" | "degraded" | "offline";
export type RelayProtocol = "websocket" | "mesh" | "invite";
export type TransportEngineKind = "mock" | "nativePreview";
export type PeerPresence = "online" | "idle" | "offline";
export type SessionSyncState = "idle" | "syncing" | "pending" | "conflict";
export type TransportRuntimeState = "inactive" | "starting" | "active";
export type TransportRuntimeDesiredState = "stopped" | "running";
export type TransportRuntimeRecoveryPolicy = "manual" | "auto";
export type TransportRuntimeQueueState = "idle" | "queued" | "backoff";
export type TransportRuntimeAdapterKind = "embedded" | "localCommand";
export type TransportRuntimeLaunchStatus = "embedded" | "ready" | "missing" | "unknown";
export type TransportRuntimeLaunchResult = "spawned" | "reused" | "failed";
export type TransportActivityKind =
  | "runtime"
  | "connect"
  | "disconnect"
  | "sync"
  | "discoverPeers"
  | "syncSessions";
export type TransportActivityLevel = "info" | "success" | "warn";
export type TransportCircleAction =
  | "connect"
  | "disconnect"
  | "sync"
  | "discoverPeers"
  | "syncSessions";
export type SettingPageId = "preferences" | "notifications" | "advanced" | "restore" | "about";
export type ThemePreference = "system" | "light" | "ink";
export type LanguagePreference = "system" | "en" | "zh-CN";
export type TextSizePreference = "compact" | "default" | "large";
export type MediaUploadDriverPreference =
  | "auto"
  | "local"
  | "filedrop"
  | "nip96"
  | "blossom"
  | "minio";

export interface UserProfile {
  name: string;
  handle: string;
  initials: string;
  status: string;
}

export interface LoginCircleSelectionInput {
  mode: LoginCircleSelectionMode;
  circleId?: string;
  inviteCode?: string;
  name?: string;
  relay?: string;
  relays?: string[];
}

export interface LoginAccessInput {
  kind: LoginAccessKind;
  value?: string;
}

export interface LoginAccessSummary {
  kind: LoginAccessKind;
  label: string;
  pubkey?: string;
}

export interface AuthSessionSummary {
  loginMethod: LoginMethod;
  access: LoginAccessSummary;
  circleSelectionMode: LoginCircleSelectionMode;
  loggedInAt: string;
}

export interface AuthRuntimeSummary {
  state: AuthRuntimeState;
  loginMethod: LoginMethod;
  accessKind: LoginAccessKind;
  label: string;
  pubkey?: string;
  error?: string;
  canSendMessages: boolean;
  sendBlockedReason?: string;
  persistedInNativeStore: boolean;
  credentialPersistedInNativeStore: boolean;
  updatedAt: string;
}

export interface AuthRuntimeBindingSummary {
  accessKind: LoginAccessKind;
  endpoint: string;
  connectionPubkey?: string;
  relayCount: number;
  hasSecret: boolean;
  requestedPermissions: string[];
  clientName?: string;
  persistedInNativeStore: boolean;
  updatedAt: string;
}

export interface AuthRuntimeClientUriSummary {
  uri: string;
  publicKey: string;
  relayCount: number;
  relays: string[];
  clientName: string;
  storedAt: string;
}

export interface UpdateAuthRuntimeInput {
  state: AuthRuntimeState;
  error?: string;
  updatedAt?: string;
  label?: string;
}

export interface LoginCompletionInput {
  method: LoginMethod;
  access: LoginAccessInput;
  userProfile: UserProfile;
  circleSelection: LoginCircleSelectionInput;
  loggedInAt?: string;
}

export interface CircleItem {
  id: string;
  name: string;
  relay: string;
  type: CircleType;
  status: CircleStatus;
  latency: string;
  description: string;
}

export interface RestoreCircleInput {
  name: string;
  relay: string;
  type: CircleType;
  description: string;
}

export interface RestorableCircleEntry extends RestoreCircleInput {
  archivedAt: string;
}

export interface ContactItem {
  id: string;
  name: string;
  initials: string;
  handle: string;
  pubkey: string;
  subtitle: string;
  bio: string;
  online?: boolean;
  blocked?: boolean;
}

export interface SessionItem {
  id: string;
  circleId: string;
  name: string;
  initials: string;
  subtitle: string;
  time: string;
  unreadCount?: number;
  muted?: boolean;
  pinned?: boolean;
  draft?: string;
  kind: SessionKind;
  category: string;
  members?: number;
  contactId?: string;
  archived?: boolean;
}

export interface MessageItem {
  id: string;
  kind: MessageKind;
  author: MessageAuthor;
  authorName?: string;
  authorContactId?: string;
  authorInitials?: string;
  body: string;
  time: string;
  meta?: string;
  deliveryStatus?: MessageDeliveryStatus;
  remoteId?: string;
  syncSource?: MessageSyncSource;
  ackedAt?: string;
  signedNostrEvent?: SignedNostrEvent;
  replyTo?: MessageReplyPreview;
}

export interface SignedNostrEvent {
  eventId: string;
  pubkey: string;
  createdAt: number;
  kind: number;
  tags: string[][];
  content: string;
  signature: string;
}

export interface MessageReplyPreview {
  messageId: string;
  remoteId?: string;
  author: MessageAuthor;
  authorLabel: string;
  kind: MessageKind;
  snippet: string;
}

export interface SettingItem {
  id: SettingPageId;
  label: string;
  icon: string;
}

export interface SettingSection {
  title: string;
  items: SettingItem[];
}

export interface GroupMember {
  contactId: string;
  role?: "admin" | "member";
}

export interface GroupProfile {
  sessionId: string;
  name: string;
  description: string;
  members: GroupMember[];
  muted?: boolean;
  canSend?: boolean;
  needsJoin?: boolean;
}

export interface AppPreferences {
  theme: ThemePreference;
  language: LanguagePreference;
  textSize: TextSizePreference;
}

export interface NotificationPreferences {
  allowSend: boolean;
  allowReceive: boolean;
  showBadge: boolean;
  archiveSummary: boolean;
  mentionsOnly: boolean;
}

export interface AdvancedPreferences {
  showMessageInfo: boolean;
  useTorNetwork: boolean;
  relayDiagnostics: boolean;
  experimentalTransport: boolean;
  mediaUploadDriver: MediaUploadDriverPreference;
  mediaUploadEndpoint: string;
}

export interface ChatDomainSeed {
  circles: CircleItem[];
  contacts: ContactItem[];
  sessions: SessionItem[];
  groups: GroupProfile[];
  messageStore: Record<string, MessageItem[]>;
}

export interface ChatDomainOverview {
  circles: CircleItem[];
  contacts: ContactItem[];
  sessions: SessionItem[];
  groups: GroupProfile[];
}

export interface PersistedShellState {
  isAuthenticated: boolean;
  authSession: AuthSessionSummary | null;
  authRuntime: AuthRuntimeSummary | null;
  authRuntimeBinding: AuthRuntimeBindingSummary | null;
  userProfile: UserProfile;
  restorableCircles: RestorableCircleEntry[];
  circles: CircleItem[];
  appPreferences: AppPreferences;
  notificationPreferences: NotificationPreferences;
  advancedPreferences: AdvancedPreferences;
  activeCircleId: string;
  selectedSessionId: string;
  sessions: SessionItem[];
  contacts: ContactItem[];
  groups: GroupProfile[];
  messageStore: Record<string, MessageItem[]>;
}

export interface ShellStateSnapshot {
  isAuthenticated: boolean;
  authSession: AuthSessionSummary | null;
  authRuntime: AuthRuntimeSummary | null;
  authRuntimeBinding: AuthRuntimeBindingSummary | null;
  userProfile: UserProfile;
  restorableCircles: RestorableCircleEntry[];
  appPreferences: AppPreferences;
  notificationPreferences: NotificationPreferences;
  advancedPreferences: AdvancedPreferences;
  activeCircleId: string;
  selectedSessionId: string;
}

export interface ChatShellSnapshot {
  domain: ChatDomainSeed;
  shell: ShellStateSnapshot;
}

export interface LoadSessionMessagesInput {
  sessionId: string;
  beforeMessageId?: string;
  limit?: number;
}

export interface ChatSessionMessagesPage {
  sessionId: string;
  messages: MessageItem[];
  hasMore: boolean;
  nextBeforeMessageId?: string;
}

export interface LoadSessionMessageUpdatesInput {
  sessionId: string;
  afterMessageId?: string;
  limit?: number;
}

export interface ChatSessionMessageUpdates {
  sessionId: string;
  messages: MessageItem[];
  hasMore: boolean;
  nextAfterMessageId?: string;
}

export interface SendMessageInput {
  sessionId: string;
  body: string;
  replyToMessageId?: string;
}

export interface SendFileMessageInput {
  sessionId: string;
  name: string;
  meta?: string;
  replyToMessageId?: string;
}

export interface SendImageMessageInput {
  sessionId: string;
  name: string;
  meta: string;
  replyToMessageId?: string;
}

export interface SendVideoMessageInput {
  sessionId: string;
  name: string;
  meta: string;
  replyToMessageId?: string;
}

export interface StoreChatMediaAssetInput {
  kind: ChatMediaKind;
  name: string;
  dataUrl: string;
}

export interface StoredChatMediaAsset {
  localPath: string;
}

export interface CleanupChatMediaAssetsResult {
  removedCount: number;
}

export interface CacheChatMessageMediaInput {
  sessionId: string;
  messageId: string;
}

export interface CachedChatMessageMediaResult {
  seed: ChatDomainSeed;
  localPath: string;
}

export interface UpdateChatMessageMediaRemoteUrlInput {
  sessionId: string;
  messageId: string;
  remoteUrl: string;
}

export interface UpdatedChatMessageMediaRemoteUrlResult {
  seed: ChatDomainSeed;
  remoteUrl: string;
}

export interface UpdateSessionDraftInput {
  sessionId: string;
  draft: string;
}

export interface UpdateMessageDeliveryStatusInput {
  sessionId: string;
  messageId: string;
  deliveryStatus: MessageDeliveryStatus;
}

export interface RetryMessageDeliveryInput {
  sessionId: string;
  messageId: string;
}

export interface MergeRemoteMessagesInput {
  sessionId: string;
  messages: MessageItem[];
}

export interface RemoteDeliveryReceipt {
  remoteId: string;
  messageId?: string;
  deliveryStatus: MessageDeliveryStatus;
  ackedAt?: string;
}

export interface MergeRemoteDeliveryReceiptsInput {
  sessionId: string;
  receipts: RemoteDeliveryReceipt[];
}

export interface StartConversationInput {
  circleId: string;
  contactId: string;
}

export interface StartSelfConversationInput {
  circleId: string;
}

export interface StartConversationResult {
  seed: ChatDomainSeed;
  sessionId: string;
}

export interface CreateGroupConversationInput {
  circleId: string;
  name: string;
  memberContactIds: string[];
}

export interface StartLookupConversationInput {
  circleId: string;
  query: string;
}

export interface SessionActionInput {
  sessionId: string;
  action: SessionAction;
}

export interface AddCircleInput {
  mode: CircleCreateMode;
  name: string;
  relay?: string;
  inviteCode?: string;
}

export interface AddCircleResult {
  seed: ChatDomainSeed;
  circleId: string;
}

export interface UpdateCircleInput {
  circleId: string;
  name: string;
  description: string;
}

export interface UpdateContactRemarkInput {
  contactId: string;
  remark: string;
}

export interface UpdateGroupNameInput {
  sessionId: string;
  name: string;
}

export interface UpdateGroupMembersInput {
  sessionId: string;
  memberContactIds: string[];
}

export interface TransportCapabilities {
  supportsMesh: boolean;
  supportsPaidRelays: boolean;
  supportsTor: boolean;
  experimentalEnabled: boolean;
}

export interface CircleTransportDiagnostic {
  circleId: string;
  relay: string;
  protocol: RelayProtocol;
  health: TransportHealth;
  latencyMs?: number;
  peerCount: number;
  queuedMessages: number;
  lastSync: string;
  reachable: boolean;
}

export interface DiscoveredPeer {
  circleId: string;
  contactId: string;
  name: string;
  handle: string;
  presence: PeerPresence;
  route: string;
  sharedSessions: number;
  lastSeen: string;
  blocked: boolean;
}

export interface SessionSyncItem {
  circleId: string;
  sessionId: string;
  sessionName: string;
  state: SessionSyncState;
  pendingMessages: number;
  source: string;
  lastMerge: string;
}

export interface TransportActivityItem {
  id: string;
  circleId: string;
  kind: TransportActivityKind;
  level: TransportActivityLevel;
  title: string;
  detail: string;
  time: string;
}

export interface TransportRuntimeSession {
  circleId: string;
  driver: string;
  adapterKind: TransportRuntimeAdapterKind;
  launchStatus: TransportRuntimeLaunchStatus;
  launchCommand?: string;
  launchArguments: string[];
  resolvedLaunchCommand?: string;
  launchError?: string;
  lastLaunchResult?: TransportRuntimeLaunchResult;
  lastLaunchPid?: number;
  lastLaunchAt?: string;
  desiredState: TransportRuntimeDesiredState;
  recoveryPolicy: TransportRuntimeRecoveryPolicy;
  queueState: TransportRuntimeQueueState;
  restartAttempts: number;
  nextRetryIn?: string;
  nextRetryAtMs?: number;
  lastFailureReason?: string;
  lastFailureAt?: string;
  state: TransportRuntimeState;
  generation: number;
  stateSince: string;
  sessionLabel: string;
  endpoint: string;
  lastEvent: string;
  lastEventAt: string;
}

export interface TransportSnapshotInput {
  activeCircleId?: string;
  useTorNetwork: boolean;
  experimentalTransport: boolean;
}

export interface TransportSnapshot {
  engine: TransportEngineKind;
  status: TransportHealth;
  activeCircleId: string;
  relayCount: number;
  connectedRelays: number;
  queuedMessages: number;
  capabilities: TransportCapabilities;
  diagnostics: CircleTransportDiagnostic[];
  peers: DiscoveredPeer[];
  sessionSync: SessionSyncItem[];
  activities: TransportActivityItem[];
  runtimeSessions: TransportRuntimeSession[];
}

export interface TransportCircleActionInput {
  circleId: string;
  action: TransportCircleAction;
  activeCircleId?: string;
  useTorNetwork: boolean;
  experimentalTransport: boolean;
  syncSinceCreatedAt?: number;
}

export interface TransportMutationResult {
  seed: ChatDomainSeed;
  snapshot: TransportSnapshot;
}
