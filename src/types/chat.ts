export type SessionKind = "direct" | "group" | "self";
export type MessageKind = "text" | "file" | "audio" | "system";
export type MessageAuthor = "me" | "peer" | "system";
export type CircleType = "default" | "paid" | "bitchat" | "custom";
export type CircleStatus = "open" | "connecting" | "closed";
export type SessionAction = "pin" | "mute" | "archive" | "delete" | "unarchive";
export type CircleCreateMode = "invite" | "private" | "custom";
export type TransportHealth = "online" | "degraded" | "offline";
export type RelayProtocol = "websocket" | "mesh" | "invite";
export type TransportEngineKind = "mock" | "nativePreview";
export type PeerPresence = "online" | "idle" | "offline";
export type SessionSyncState = "idle" | "syncing" | "pending" | "conflict";
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

export interface UserProfile {
  name: string;
  handle: string;
  initials: string;
  status: string;
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
  body: string;
  time: string;
  meta?: string;
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
}

export interface ChatDomainSeed {
  circles: CircleItem[];
  contacts: ContactItem[];
  sessions: SessionItem[];
  groups: GroupProfile[];
  messageStore: Record<string, MessageItem[]>;
}

export interface PersistedShellState {
  isAuthenticated: boolean;
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

export interface SendMessageInput {
  sessionId: string;
  body: string;
}

export interface StartConversationInput {
  circleId: string;
  contactId: string;
}

export interface StartConversationResult {
  seed: ChatDomainSeed;
  sessionId: string;
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
}

export interface TransportCircleActionInput {
  circleId: string;
  action: TransportCircleAction;
  activeCircleId?: string;
  useTorNetwork: boolean;
  experimentalTransport: boolean;
}

export interface TransportMutationResult {
  seed: ChatDomainSeed;
  snapshot: TransportSnapshot;
}
