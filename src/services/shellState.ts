import type {
  AdvancedPreferences,
  AuthSessionSummary,
  AppPreferences,
  NotificationPreferences,
  PersistedShellState,
  UserProfile,
} from "../types/chat";
import { defaultUserProfile } from "../data/shellDefaults";

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
    },
    circleSelectionMode,
    loggedInAt,
  };
}

function normalizeShellState(
  value: Partial<PersistedShellState> | null | undefined,
  defaults: PersistedShellState,
): PersistedShellState {
  return {
    isAuthenticated:
      typeof value?.isAuthenticated === "boolean"
        ? value.isAuthenticated
        : defaults.isAuthenticated,
    authSession: normalizeAuthSession(value?.authSession),
    userProfile: normalizeUserProfile(value?.userProfile, defaults.userProfile ?? defaultUserProfile),
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
