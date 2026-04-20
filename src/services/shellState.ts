import type {
  AdvancedPreferences,
  AuthRuntimeBindingSummary,
  AuthRuntimeSummary,
  AuthSessionSummary,
  AppPreferences,
  NotificationPreferences,
  PersistedShellState,
  RestorableCircleEntry,
  UserProfile,
} from "../types/chat";
import { defaultUserProfile } from "../data/shellDefaults";
import {
  deriveAuthRuntimeFromAuthSession,
  resolveAuthRuntimeCanSendMessages,
  resolveAuthRuntimeSendBlockedReason,
} from "./authRuntime";

const LOCAL_STORAGE_KEY = "p2p-chat.shell-state";

function cloneState<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function readLocalShellState(): Partial<PersistedShellState> | null {
  try {
    const raw = window.localStorage.getItem(LOCAL_STORAGE_KEY);
    if (!raw) {
      return null;
    }

    return JSON.parse(raw) as Partial<PersistedShellState>;
  } catch {
    return null;
  }
}

function writeLocalShellState(state: PersistedShellState) {
  try {
    window.localStorage.setItem(LOCAL_STORAGE_KEY, JSON.stringify(state));
  } catch {
    // Ignore storage failures and keep the in-memory shell responsive.
  }
}

export function persistShellStateLocally(state: PersistedShellState) {
  writeLocalShellState(cloneState(state));
}

function normalizeMessageStore(
  value: Partial<PersistedShellState>["messageStore"],
  fallback: PersistedShellState["messageStore"],
) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return cloneState(fallback);
  }

  const normalizedEntries = Object.entries(value).map(([sessionId, messages]) => {
    return [sessionId, Array.isArray(messages) ? cloneState(messages) : []];
  });

  return Object.fromEntries(normalizedEntries);
}

function normalizeAppPreferences(
  value: Partial<AppPreferences> | null | undefined,
  fallback: AppPreferences,
): AppPreferences {
  return {
    theme: value?.theme === "light" || value?.theme === "ink" || value?.theme === "system"
      ? value.theme
      : fallback.theme,
    language: value?.language === "en" || value?.language === "zh-CN" || value?.language === "system"
      ? value.language
      : fallback.language,
    textSize:
      value?.textSize === "compact" || value?.textSize === "large" || value?.textSize === "default"
        ? value.textSize
        : fallback.textSize,
  };
}

function normalizeNotificationPreferences(
  value: Partial<NotificationPreferences> | null | undefined,
  fallback: NotificationPreferences,
): NotificationPreferences {
  return {
    allowSend: typeof value?.allowSend === "boolean" ? value.allowSend : fallback.allowSend,
    allowReceive: typeof value?.allowReceive === "boolean" ? value.allowReceive : fallback.allowReceive,
    showBadge: typeof value?.showBadge === "boolean" ? value.showBadge : fallback.showBadge,
    archiveSummary:
      typeof value?.archiveSummary === "boolean" ? value.archiveSummary : fallback.archiveSummary,
    mentionsOnly: typeof value?.mentionsOnly === "boolean" ? value.mentionsOnly : fallback.mentionsOnly,
  };
}

function normalizeAdvancedPreferences(
  value: Partial<AdvancedPreferences> | null | undefined,
  fallback: AdvancedPreferences,
): AdvancedPreferences {
  return {
    showMessageInfo:
      typeof value?.showMessageInfo === "boolean" ? value.showMessageInfo : fallback.showMessageInfo,
    useTorNetwork: typeof value?.useTorNetwork === "boolean" ? value.useTorNetwork : fallback.useTorNetwork,
    relayDiagnostics:
      typeof value?.relayDiagnostics === "boolean" ? value.relayDiagnostics : fallback.relayDiagnostics,
    experimentalTransport:
      typeof value?.experimentalTransport === "boolean"
        ? value.experimentalTransport
        : fallback.experimentalTransport,
  };
}

function normalizeUserProfile(
  value: Partial<UserProfile> | null | undefined,
  fallback: UserProfile,
): UserProfile {
  return {
    name: typeof value?.name === "string" && value.name.trim() ? value.name : fallback.name,
    handle:
      typeof value?.handle === "string" && value.handle.trim() ? value.handle : fallback.handle,
    initials:
      typeof value?.initials === "string" && value.initials.trim()
        ? value.initials.slice(0, 2).toUpperCase()
        : fallback.initials,
    status:
      typeof value?.status === "string" && value.status.trim() ? value.status : fallback.status,
  };
}

function normalizeAuthSession(
  value: Partial<AuthSessionSummary> | null | undefined,
): AuthSessionSummary | null {
  if (!value || typeof value !== "object") {
    return null;
  }

  const loginMethod =
    value.loginMethod === "quickStart" ||
    value.loginMethod === "existingAccount" ||
    value.loginMethod === "signer"
      ? value.loginMethod
      : null;
  const circleSelectionMode =
    value.circleSelectionMode === "existing" ||
    value.circleSelectionMode === "invite" ||
    value.circleSelectionMode === "custom" ||
    value.circleSelectionMode === "restore"
      ? value.circleSelectionMode
      : null;
  const accessKind =
    value.access?.kind === "localProfile" ||
    value.access?.kind === "nsec" ||
    value.access?.kind === "npub" ||
    value.access?.kind === "hexKey" ||
    value.access?.kind === "bunker" ||
    value.access?.kind === "nostrConnect"
      ? value.access.kind
      : null;
  const accessLabel =
    typeof value.access?.label === "string" && value.access.label.trim()
      ? value.access.label
      : null;
  const accessPubkey =
    typeof value.access?.pubkey === "string" && value.access.pubkey.trim()
      ? value.access.pubkey.trim()
      : undefined;
  const loggedInAt =
    typeof value.loggedInAt === "string" && value.loggedInAt.trim()
      ? value.loggedInAt
      : null;

  if (!loginMethod || !circleSelectionMode || !accessKind || !accessLabel || !loggedInAt) {
    return null;
  }

  return {
    loginMethod,
    access: {
      kind: accessKind,
      label: accessLabel,
      pubkey: accessPubkey,
    },
    circleSelectionMode,
    loggedInAt,
  };
}

function normalizeAuthRuntime(
  value: Partial<AuthRuntimeSummary> | null | undefined,
): AuthRuntimeSummary | null {
  if (!value || typeof value !== "object") {
    return null;
  }

  const state =
    value.state === "localProfile" ||
    value.state === "pending" ||
    value.state === "connected" ||
    value.state === "failed"
      ? value.state
      : null;
  const loginMethod =
    value.loginMethod === "quickStart" ||
    value.loginMethod === "existingAccount" ||
    value.loginMethod === "signer"
      ? value.loginMethod
      : null;
  const accessKind =
    value.accessKind === "localProfile" ||
    value.accessKind === "nsec" ||
    value.accessKind === "npub" ||
    value.accessKind === "hexKey" ||
    value.accessKind === "bunker" ||
    value.accessKind === "nostrConnect"
      ? value.accessKind
      : null;
  const label = typeof value.label === "string" && value.label.trim() ? value.label : null;
  const pubkey =
    typeof value.pubkey === "string" && value.pubkey.trim() ? value.pubkey.trim() : undefined;
  const canSendMessages =
    typeof value.canSendMessages === "boolean" ? value.canSendMessages : null;
  const sendBlockedReason =
    typeof value.sendBlockedReason === "string" && value.sendBlockedReason.trim()
      ? value.sendBlockedReason.trim()
      : undefined;
  const persistedInNativeStore =
    typeof value.persistedInNativeStore === "boolean" ? value.persistedInNativeStore : false;
  const credentialPersistedInNativeStore =
    typeof value.credentialPersistedInNativeStore === "boolean"
      ? value.credentialPersistedInNativeStore
      : false;
  const updatedAt = typeof value.updatedAt === "string" && value.updatedAt.trim() ? value.updatedAt : null;
  const error = typeof value.error === "string" && value.error.trim() ? value.error : undefined;

  if (!state || !loginMethod || !accessKind || !label || !updatedAt) {
    return null;
  }

  return {
    state,
    loginMethod,
    accessKind,
    label,
    pubkey,
    error,
    canSendMessages:
      canSendMessages ??
      resolveAuthRuntimeCanSendMessages(accessKind, state, error),
    sendBlockedReason:
      sendBlockedReason ??
      resolveAuthRuntimeSendBlockedReason(accessKind, state, error),
    persistedInNativeStore,
    credentialPersistedInNativeStore,
    updatedAt,
  };
}

function normalizeAuthRuntimeBinding(
  value: Partial<AuthRuntimeBindingSummary> | null | undefined,
): AuthRuntimeBindingSummary | null {
  if (!value || typeof value !== "object") {
    return null;
  }

  const accessKind =
    value.accessKind === "bunker" || value.accessKind === "nostrConnect"
      ? value.accessKind
      : null;
  const endpoint = typeof value.endpoint === "string" && value.endpoint.trim() ? value.endpoint : null;
  const connectionPubkey =
    typeof value.connectionPubkey === "string" && value.connectionPubkey.trim()
      ? value.connectionPubkey.trim()
      : undefined;
  const relayCount = typeof value.relayCount === "number" && value.relayCount >= 0
    ? Math.floor(value.relayCount)
    : 0;
  const hasSecret = typeof value.hasSecret === "boolean" ? value.hasSecret : false;
  const requestedPermissions = Array.isArray(value.requestedPermissions)
    ? value.requestedPermissions
        .filter((permission): permission is string => typeof permission === "string")
        .map((permission) => permission.trim())
        .filter(Boolean)
    : [];
  const clientName =
    typeof value.clientName === "string" && value.clientName.trim()
      ? value.clientName.trim()
      : undefined;
  const persistedInNativeStore =
    typeof value.persistedInNativeStore === "boolean" ? value.persistedInNativeStore : null;
  const updatedAt = typeof value.updatedAt === "string" && value.updatedAt.trim() ? value.updatedAt : null;

  if (!accessKind || !endpoint || persistedInNativeStore === null || !updatedAt) {
    return null;
  }

  return {
    accessKind,
    endpoint,
    connectionPubkey,
    relayCount,
    hasSecret,
    requestedPermissions,
    clientName,
    persistedInNativeStore,
    updatedAt,
  };
}

function normalizeRestorableCircles(
  value: Partial<RestorableCircleEntry>[] | null | undefined,
): RestorableCircleEntry[] {
  if (!Array.isArray(value)) {
    return [];
  }

  return value
    .map((entry) => {
      const type =
        entry?.type === "default" ||
        entry?.type === "paid" ||
        entry?.type === "bitchat" ||
        entry?.type === "custom"
          ? entry.type
          : null;
      const name = typeof entry?.name === "string" ? entry.name.trim() : "";
      const relay = typeof entry?.relay === "string" ? entry.relay.trim() : "";
      const description = typeof entry?.description === "string" ? entry.description.trim() : "";
      const archivedAt = typeof entry?.archivedAt === "string" ? entry.archivedAt.trim() : "";

      if (!type || !name || !relay || !archivedAt) {
        return null;
      }

      return {
        type,
        name,
        relay,
        description,
        archivedAt,
      };
    })
    .filter((entry): entry is RestorableCircleEntry => !!entry);
}

function normalizeShellState(
  value: Partial<PersistedShellState> | null | undefined,
  defaults: PersistedShellState,
): PersistedShellState {
  const authSession = normalizeAuthSession(value?.authSession);
  const authRuntime =
    normalizeAuthRuntime(value?.authRuntime) ?? deriveAuthRuntimeFromAuthSession(authSession);

  return {
    isAuthenticated:
      typeof value?.isAuthenticated === "boolean"
        ? value.isAuthenticated
        : defaults.isAuthenticated,
    authSession,
    authRuntime,
    authRuntimeBinding: normalizeAuthRuntimeBinding(value?.authRuntimeBinding),
    userProfile: normalizeUserProfile(value?.userProfile, defaults.userProfile ?? defaultUserProfile),
    restorableCircles: normalizeRestorableCircles(value?.restorableCircles),
    circles: Array.isArray(value?.circles) ? cloneState(value.circles) : cloneState(defaults.circles),
    appPreferences: normalizeAppPreferences(value?.appPreferences, defaults.appPreferences),
    notificationPreferences: normalizeNotificationPreferences(
      value?.notificationPreferences,
      defaults.notificationPreferences,
    ),
    advancedPreferences: normalizeAdvancedPreferences(
      value?.advancedPreferences,
      defaults.advancedPreferences,
    ),
    activeCircleId:
      typeof value?.activeCircleId === "string"
        ? value.activeCircleId
        : defaults.activeCircleId,
    selectedSessionId:
      typeof value?.selectedSessionId === "string"
        ? value.selectedSessionId
        : defaults.selectedSessionId,
    sessions: Array.isArray(value?.sessions) ? cloneState(value.sessions) : cloneState(defaults.sessions),
    contacts: Array.isArray(value?.contacts) ? cloneState(value.contacts) : cloneState(defaults.contacts),
    groups: Array.isArray(value?.groups) ? cloneState(value.groups) : cloneState(defaults.groups),
    messageStore: normalizeMessageStore(value?.messageStore, defaults.messageStore),
  };
}

export function loadLocalShellState(defaults: PersistedShellState): PersistedShellState {
  return normalizeShellState(readLocalShellState(), defaults);
}
