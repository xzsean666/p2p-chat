import type {
  AdvancedPreferences,
  AppPreferences,
  ChatShellSnapshot,
  ChatDomainSeed,
  NotificationPreferences,
  PersistedShellState,
  ShellStateSnapshot,
  UserProfile,
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
  showMessageInfo: true,
  useTorNetwork: false,
  relayDiagnostics: true,
  experimentalTransport: true,
  mediaUploadDriver: "auto",
  mediaUploadEndpoint: "",
};

export const defaultUserProfile: UserProfile = {
  name: "Sean Chen",
  handle: "@seanchen",
  initials: "SC",
  status: "Circle owner",
};

export function createEmptyShellState(): PersistedShellState {
  return cloneState({
    isAuthenticated: false,
    authSession: null,
    authRuntime: null,
    authRuntimeBinding: null,
    userProfile: defaultUserProfile,
    restorableCircles: [],
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

export function shellStateSnapshotFromPersistedState(
  state: PersistedShellState,
): ShellStateSnapshot {
  return cloneState({
    isAuthenticated: state.isAuthenticated,
    authSession: state.authSession,
    authRuntime: state.authRuntime,
    authRuntimeBinding: state.authRuntimeBinding,
    userProfile: state.userProfile,
    restorableCircles: state.restorableCircles,
    appPreferences: state.appPreferences,
    notificationPreferences: state.notificationPreferences,
    advancedPreferences: state.advancedPreferences,
    activeCircleId: state.activeCircleId,
    selectedSessionId: state.selectedSessionId,
  });
}

export function createChatShellSnapshotFromPersistedState(
  state: PersistedShellState,
): ChatShellSnapshot {
  return cloneState({
    domain: {
      circles: state.circles,
      contacts: state.contacts,
      sessions: state.sessions,
      groups: state.groups,
      messageStore: state.messageStore,
    },
    shell: shellStateSnapshotFromPersistedState(state),
  });
}

export function createPersistedShellStateFromChatShellSnapshot(
  snapshot: ChatShellSnapshot,
): PersistedShellState {
  return cloneState({
    ...createShellStateFromDomainSeed(snapshot.domain),
    isAuthenticated: snapshot.shell.isAuthenticated,
    authSession: snapshot.shell.authSession,
    authRuntime: snapshot.shell.authRuntime,
    authRuntimeBinding: snapshot.shell.authRuntimeBinding,
    userProfile: snapshot.shell.userProfile,
    restorableCircles: snapshot.shell.restorableCircles,
    appPreferences: snapshot.shell.appPreferences,
    notificationPreferences: snapshot.shell.notificationPreferences,
    advancedPreferences: snapshot.shell.advancedPreferences,
    activeCircleId: snapshot.shell.activeCircleId,
    selectedSessionId: snapshot.shell.selectedSessionId,
  });
}

export function createLoggedOutShellState(
  state: Pick<
    PersistedShellState,
    "restorableCircles" | "appPreferences" | "notificationPreferences" | "advancedPreferences"
  >,
): PersistedShellState {
  return cloneState({
    ...createEmptyShellState(),
    restorableCircles: state.restorableCircles,
    appPreferences: state.appPreferences,
    notificationPreferences: state.notificationPreferences,
    advancedPreferences: state.advancedPreferences,
  });
}
