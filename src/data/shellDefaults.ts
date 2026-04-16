import type {
  AdvancedPreferences,
  AppPreferences,
  ChatDomainSeed,
  NotificationPreferences,
  PersistedShellState,
} from "../types/chat";

function cloneState<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

export const defaultAppPreferences: AppPreferences = {
  theme: "system",
  language: "en",
  textSize: "default",
};

export const defaultNotificationPreferences: NotificationPreferences = {
  allowSend: true,
  allowReceive: false,
  showBadge: true,
  archiveSummary: true,
  mentionsOnly: false,
};

export const defaultAdvancedPreferences: AdvancedPreferences = {
  showMessageInfo: false,
  useTorNetwork: false,
  relayDiagnostics: true,
  experimentalTransport: false,
};

export function createEmptyShellState(): PersistedShellState {
  return cloneState({
    isAuthenticated: false,
    circles: [],
    appPreferences: defaultAppPreferences,
    notificationPreferences: defaultNotificationPreferences,
    advancedPreferences: defaultAdvancedPreferences,
    activeCircleId: "",
    selectedSessionId: "",
    sessions: [],
    contacts: [],
    groups: [],
    messageStore: {},
  });
}

export function createShellStateFromDomainSeed(seed: ChatDomainSeed): PersistedShellState {
  return cloneState({
    ...createEmptyShellState(),
    circles: seed.circles,
    activeCircleId: seed.circles[0]?.id ?? "",
    sessions: seed.sessions,
    contacts: seed.contacts,
    groups: seed.groups,
    messageStore: seed.messageStore,
  });
}
