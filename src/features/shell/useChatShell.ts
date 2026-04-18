import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import {
  createChatShellSnapshotFromPersistedState,
  createEmptyShellState,
  createLoggedOutShellState,
} from "../../data/shellDefaults";
import { createChatSeedFallback } from "../../mock/chatSeedFallback";
import {
  addChatCircle,
  applyChatSessionAction,
  createGroupConversation,
  removeChatCircle,
  retryChatMessageDelivery,
  sendChatMessage,
  startDirectConversation,
  startLookupConversation,
  startSelfConversation,
  toggleChatContactBlock,
  updateChatGroupMembers,
  updateChatGroupName,
  updateChatSessionDraft,
  updateChatCircle,
} from "../../services/chatMutations";
import {
  loadChatDomainOverview,
  loadChatSessionMessageUpdates,
  loadChatSessionMessages,
  loadChatShellSnapshot,
  persistChatShellSnapshotLocally,
  saveChatShellSnapshot,
} from "../../services/chatShell";
import {
  applyTransportCircleAction,
  deriveRuntimeRecoveryAction,
  loadTransportSnapshot,
} from "../../services/transportDiagnostics";
import {
  cloneOverlayPages,
  createOverlayHistoryState,
  overlayRouteHash,
  parseOverlayHistoryState,
  parseOverlayRouteHash,
  type OverlayPage,
} from "./overlayRoutes";
import type {
  AdvancedPreferences,
  AddCircleInput,
  AppPreferences,
  AuthSessionSummary,
  ChatDomainOverview,
  ChatDomainSeed,
  ChatShellSnapshot,
  CircleStatus,
  ContactItem,
  CreateGroupConversationInput,
  DiscoveredPeer,
  GroupProfile,
  LoginCircleSelectionInput,
  LoginCompletionInput,
  MessageItem,
  NotificationPreferences,
  PersistedShellState,
  ShellStateSnapshot,
  SessionSyncItem,
  SettingPageId,
  SessionAction,
  SessionItem,
  TransportActivityItem,
  TransportCircleAction,
  TransportMutationResult,
  TransportRuntimeSession,
  TransportSnapshot,
  UpdateCircleInput,
  UpdateGroupMembersInput,
  UpdateGroupNameInput,
  UserProfile,
} from "../../types/chat";

type BootstrapStatus = {
  project: string;
  phase: string;
  ready: boolean;
  stack: string[];
  next: string[];
};

type TransportNotice = {
  id: string;
  tone: "info" | "warn";
  title: string;
  detail: string;
  circleId?: string;
};

type SessionMessagePageState = {
  initialized: boolean;
  loading: boolean;
  hasMore: boolean;
  nextBeforeMessageId?: string;
};

type DomainSeedMessageMode = "full" | "preview";

const SESSION_MESSAGE_PAGE_SIZE = 30;

function cloneState<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

export function useChatShell() {
  const initialShellState = createEmptyShellState();
  const isShellStateReady = ref(false);
  let persistTimer: ReturnType<typeof window.setTimeout> | null = null;
  let draftPersistTimer: ReturnType<typeof window.setTimeout> | null = null;
  let transportTimer: ReturnType<typeof window.setTimeout> | null = null;
  let transportHeartbeatTimer: ReturnType<typeof window.setInterval> | null = null;
  let transportNoticeTimer: ReturnType<typeof window.setTimeout> | null = null;
  let activeTransportHeartbeatIntervalMs = 0;
  let latestDraftMutationSerial = 0;
  let pendingDraftSessionId: string | null = null;
  let pendingDraftValue = "";
  const transportBusyCircleId = ref("");

  const searchText = ref("");
  const composerText = ref("");
  const isAuthenticated = ref(initialShellState.isAuthenticated);
  const authSession = ref(initialShellState.authSession);
  const userProfile = ref<UserProfile>(initialShellState.userProfile);
  const showLaunch = ref(true);
  const showCircleSwitcher = ref(false);
  const showSettingsDrawer = ref(false);
  const showDetailsDrawer = ref(false);
  const circles = ref(initialShellState.circles);
  const appPreferences = ref(initialShellState.appPreferences);
  const notificationPreferences = ref(initialShellState.notificationPreferences);
  const advancedPreferences = ref(initialShellState.advancedPreferences);
  const activeCircleId = ref(initialShellState.activeCircleId);
  const selectedSessionId = ref(initialShellState.selectedSessionId);
  const bootstrapStatus = ref<BootstrapStatus | null>(null);
  const sessions = ref<SessionItem[]>(initialShellState.sessions);
  const contacts = ref<ContactItem[]>(initialShellState.contacts);
  const groups = ref<GroupProfile[]>(initialShellState.groups);
  const messageStore = ref<Record<string, MessageItem[]>>(initialShellState.messageStore);
  const sessionMessagePages = ref<Record<string, SessionMessagePageState>>({});
  const overlayPages = ref<OverlayPage[]>([]);
  const transportSnapshot = ref<TransportSnapshot | null>(null);
  const transportNotice = ref<TransportNotice | null>(null);
  let overlayNavigationReady = false;
  let overlayHistoryDepth = 0;
  let popstateHandler: ((event: PopStateEvent) => void) | null = null;
  let pagehideHandler: (() => void) | null = null;
  let visibilitychangeHandler: (() => void) | null = null;
  let systemThemeMediaQuery: MediaQueryList | null = null;
  let systemThemeChangeHandler: (() => void) | null = null;

  const activeCircle = computed(() => {
    return circles.value.find((circle) => circle.id === activeCircleId.value) ?? null;
  });

  const sessionsForCircle = computed(() => {
    return sessions.value.filter((session) => session.circleId === activeCircleId.value);
  });

  const visibleSessionsForCircle = computed(() => {
    return sessionsForCircle.value.filter((session) => !session.archived);
  });

  const archivedSessionsForCircle = computed(() => {
    return sessionsForCircle.value.filter((session) => session.archived);
  });

  const currentCircleContactIds = computed(() => {
    return sessionsForCircle.value
      .map((session) => session.contactId)
      .filter((contactId): contactId is string => !!contactId);
  });

  const orderedVisibleSessions = computed(() => {
    return visibleSessionsForCircle.value
      .map((session, index) => ({ session, index }))
      .sort((left, right) => {
        if (!!left.session.pinned !== !!right.session.pinned) {
          return Number(!!right.session.pinned) - Number(!!left.session.pinned);
        }

        return left.index - right.index;
      })
      .map((entry) => entry.session);
  });

  const filteredSessions = computed(() => {
    const keyword = searchText.value.trim().toLowerCase();
    if (!keyword) {
      return orderedVisibleSessions.value;
    }

    return orderedVisibleSessions.value.filter((session) => {
      return [session.name, session.subtitle, session.category, session.draft || ""]
        .join(" ")
        .toLowerCase()
        .includes(keyword);
    });
  });

  const selectedSession = computed(() => {
    return (
      visibleSessionsForCircle.value.find((session) => session.id === selectedSessionId.value) ??
      visibleSessionsForCircle.value[0] ??
      null
    );
  });

  const activeMessages = computed(() => {
    if (!selectedSession.value) {
      return [];
    }

    return messageStore.value[selectedSession.value.id] ?? [];
  });

  const activeMessagePageState = computed<SessionMessagePageState>(() => {
    if (!selectedSession.value) {
      return {
        initialized: true,
        loading: false,
        hasMore: false,
      };
    }

    return (
      sessionMessagePages.value[selectedSession.value.id] ?? {
        initialized: false,
        loading: false,
        hasMore: false,
      }
    );
  });

  const canLoadOlderMessages = computed(() => {
    return !!selectedSession.value && activeMessagePageState.value.hasMore;
  });

  const loadingOlderMessages = computed(() => {
    return !!selectedSession.value && activeMessagePageState.value.loading;
  });

  const selectedContact = computed(() => {
    if (!selectedSession.value?.contactId) {
      return null;
    }

    return contacts.value.find((item) => item.id === selectedSession.value?.contactId) ?? null;
  });

  const selectedGroup = computed(() => {
    if (selectedSession.value?.kind !== "group") {
      return null;
    }

    return groups.value.find((group) => group.sessionId === selectedSession.value?.id) ?? null;
  });

  const selectedGroupMembers = computed(() => {
    if (!selectedGroup.value) {
      return [];
    }

    return selectedGroup.value.members
      .map((member) => contacts.value.find((item) => item.id === member.contactId))
      .filter((item): item is ContactItem => !!item);
  });

  const activeOverlayPage = computed(() => {
    return overlayPages.value[overlayPages.value.length - 1] ?? null;
  });

  const activeOverlayContact = computed(() => {
    const page = activeOverlayPage.value;
    if (page?.kind !== "contact") {
      return null;
    }

    return contacts.value.find((item) => item.id === page.contactId) ?? null;
  });

  const activeOverlayCircle = computed(() => {
    const page = activeOverlayPage.value;
    if (page?.kind !== "circle-detail") {
      return null;
    }

    return circles.value.find((circle) => circle.id === page.circleId) ?? null;
  });

  const activeTransportDiagnostic = computed(() => {
    return (
      transportSnapshot.value?.diagnostics.find((item) => item.circleId === activeCircleId.value) ?? null
    );
  });

  const activeOverlayTransportDiagnostic = computed(() => {
    if (!activeOverlayCircle.value) {
      return null;
    }

    return (
      transportSnapshot.value?.diagnostics.find((item) => item.circleId === activeOverlayCircle.value?.id) ??
      null
    );
  });

  const activeOverlayDiscoveredPeers = computed<DiscoveredPeer[]>(() => {
    if (!activeOverlayCircle.value) {
      return [];
    }

    return transportSnapshot.value?.peers.filter((item) => item.circleId === activeOverlayCircle.value?.id) ?? [];
  });

  const activeOverlaySessionSyncItems = computed<SessionSyncItem[]>(() => {
    if (!activeOverlayCircle.value) {
      return [];
    }

    return (
      transportSnapshot.value?.sessionSync.filter((item) => item.circleId === activeOverlayCircle.value?.id) ?? []
    );
  });

  const activeOverlayTransportActivities = computed<TransportActivityItem[]>(() => {
    if (!activeOverlayCircle.value) {
      return [];
    }

    return (
      transportSnapshot.value?.activities.filter((item) => item.circleId === activeOverlayCircle.value?.id) ?? []
    );
  });

  const activeOverlayRuntimeSessions = computed(() => {
    if (!activeOverlayCircle.value) {
      return [];
    }

    return (
      transportSnapshot.value?.runtimeSessions.filter((item) => item.circleId === activeOverlayCircle.value?.id) ?? []
    );
  });

  const isActiveOverlayTransportBusy = computed(() => {
    return !!activeOverlayCircle.value && transportBusyCircleId.value === activeOverlayCircle.value.id;
  });

  const activeOverlayCircleSessions = computed(() => {
    if (!activeOverlayCircle.value) {
      return [];
    }

    return sessions.value.filter((session) => session.circleId === activeOverlayCircle.value?.id);
  });

  const activeOverlayCircleSessionCount = computed(() => {
    return activeOverlayCircleSessions.value.length;
  });

  const activeOverlayCircleDirectCount = computed(() => {
    return activeOverlayCircleSessions.value.filter((session) => session.kind !== "group").length;
  });

  const activeOverlayCircleGroupCount = computed(() => {
    return activeOverlayCircleSessions.value.filter((session) => session.kind === "group").length;
  });

  const activeOverlayCircleArchivedCount = computed(() => {
    return activeOverlayCircleSessions.value.filter((session) => session.archived).length;
  });

  function overlayGroupSessionId(page: OverlayPage | null) {
    switch (page?.kind) {
      case "group":
      case "group-name":
      case "group-members":
      case "group-add-members":
      case "group-remove-members":
        return page.sessionId;
      default:
        return null;
    }
  }

  const activeOverlayGroupSession = computed(() => {
    const sessionId = overlayGroupSessionId(activeOverlayPage.value);
    if (!sessionId) {
      return null;
    }

    return sessions.value.find((session) => session.id === sessionId) ?? null;
  });

  const activeOverlayGroup = computed(() => {
    const sessionId = overlayGroupSessionId(activeOverlayPage.value);
    if (!sessionId) {
      return null;
    }

    return groups.value.find((group) => group.sessionId === sessionId) ?? null;
  });

  const activeOverlayGroupMembers = computed(() => {
    if (!activeOverlayGroup.value) {
      return [];
    }

    return activeOverlayGroup.value.members
      .map((member) => contacts.value.find((item) => item.id === member.contactId))
      .filter((item): item is ContactItem => !!item);
  });

  const activeOverlayGroupAvailableContacts = computed(() => {
    if (!activeOverlayGroup.value) {
      return [];
    }

    const memberIds = new Set(activeOverlayGroup.value.members.map((member) => member.contactId));
    return contacts.value.filter((contact) => !memberIds.has(contact.id));
  });

  const activeOverlayGroupCreateContacts = computed(() => {
    const page = activeOverlayPage.value;
    if (page?.kind !== "group-create") {
      return [];
    }

    const memberIds = Array.from(new Set(page.memberContactIds));
    return memberIds
      .map((contactId) => contacts.value.find((contact) => contact.id === contactId))
      .filter((contact): contact is ContactItem => !!contact);
  });

  const inviteLink = computed(() => {
    if (!activeCircle.value) {
      return "p2pchat://circle";
    }

    return `p2pchat://circle/${activeCircle.value.id}?relay=${encodeURIComponent(activeCircle.value.relay)}`;
  });

  function clearTransportNoticeTimer() {
    if (!transportNoticeTimer) {
      return;
    }

    window.clearTimeout(transportNoticeTimer);
    transportNoticeTimer = null;
  }

  function dismissTransportNotice() {
    clearTransportNoticeTimer();
    transportNotice.value = null;
  }

  function showTransportNotice(notice: TransportNotice, durationMs = notice.tone === "warn" ? 10_000 : 7_000) {
    if (transportNotice.value?.id === notice.id) {
      return;
    }

    transportNotice.value = notice;
    clearTransportNoticeTimer();
    transportNoticeTimer = window.setTimeout(() => {
      if (transportNotice.value?.id === notice.id) {
        transportNotice.value = null;
      }

      transportNoticeTimer = null;
    }, durationMs);
  }

  function circleLabelForRuntimeNotice(circleId: string) {
    return circles.value.find((circle) => circle.id === circleId)?.name ?? circleId;
  }

  function buildRuntimeSessionNotice(
    previous: TransportRuntimeSession | undefined,
    session: TransportRuntimeSession,
  ): TransportNotice | null {
    const circleLabel = circleLabelForRuntimeNotice(session.circleId);

    if (session.launchStatus === "missing" && previous?.launchStatus !== "missing") {
      return {
        id: `runtime-missing-${session.circleId}-${session.launchCommand ?? "command"}`,
        tone: "warn",
        title: `${circleLabel} runtime command unavailable`,
        detail:
          session.launchError ??
          `command \`${session.launchCommand ?? "local runtime command"}\` is not available`,
        circleId: session.circleId,
      };
    }

    const launchFailed =
      session.lastLaunchResult === "failed" &&
      (previous?.lastLaunchAt !== session.lastLaunchAt ||
        previous?.lastLaunchResult !== session.lastLaunchResult ||
        previous?.launchError !== session.launchError);
    if (launchFailed) {
      return {
        id: `runtime-launch-failed-${session.circleId}-${session.lastLaunchAt ?? session.lastEventAt}`,
        tone: "warn",
        title: `${circleLabel} runtime launch failed`,
        detail: session.launchError ?? session.lastFailureReason ?? session.lastEvent,
        circleId: session.circleId,
      };
    }

    if (session.lastFailureReason && previous?.lastFailureReason !== session.lastFailureReason) {
      return {
        id: `runtime-failure-${session.circleId}-${session.lastFailureAt ?? session.lastEventAt}`,
        tone: "warn",
        title: session.lastEvent.includes("exited")
          ? `${circleLabel} runtime exited`
          : `${circleLabel} runtime needs recovery`,
        detail: session.lastFailureReason,
        circleId: session.circleId,
      };
    }

    return null;
  }

  function maybeShowTransportNoticeFromSnapshot(
    previousSnapshot: TransportSnapshot | null,
    snapshot: TransportSnapshot,
  ) {
    if (!previousSnapshot) {
      return;
    }

    const previousRuntimeSessions = new Map(
      previousSnapshot.runtimeSessions.map((session) => [session.circleId, session]),
    );

    for (const session of snapshot.runtimeSessions) {
      const notice = buildRuntimeSessionNotice(previousRuntimeSessions.get(session.circleId), session);
      if (notice) {
        showTransportNotice(notice);
        return;
      }
    }
  }

  function closeTransientChrome() {
    showCircleSwitcher.value = false;
    showSettingsDrawer.value = false;
    showDetailsDrawer.value = false;
  }

  function overlayPageExists(page: OverlayPage) {
    if (!isAuthenticated.value) {
      return false;
    }

    switch (page.kind) {
      case "circle-directory":
      case "settings-detail":
      case "new-message":
      case "find-people":
      case "group-select-members":
      case "archived":
        return true;
      case "group-create":
        return (
          Array.isArray(page.memberContactIds) &&
          page.memberContactIds.length > 0 &&
          page.memberContactIds.every((contactId) => contacts.value.some((contact) => contact.id === contactId))
        );
      case "circle-detail":
        return circles.value.some((circle) => circle.id === page.circleId);
      case "contact":
        return contacts.value.some((contact) => contact.id === page.contactId);
      case "group":
      case "group-name":
      case "group-members":
      case "group-add-members":
      case "group-remove-members":
        return (
          sessions.value.some((session) => session.id === page.sessionId) &&
          groups.value.some((group) => group.sessionId === page.sessionId)
        );
    }
  }

  function sanitizeOverlayPages(pages: OverlayPage[]) {
    return pages.filter((page) => overlayPageExists(page));
  }

  function overlayPagesMatch(left: OverlayPage[], right: OverlayPage[]) {
    if (left.length !== right.length) {
      return false;
    }

    return left.every((page, index) => {
      return JSON.stringify(page) === JSON.stringify(right[index]);
    });
  }

  function overlayNavigationUrl(pages: OverlayPage[]) {
    if (typeof window === "undefined") {
      return "";
    }

    return `${window.location.pathname}${window.location.search}${overlayRouteHash(pages)}`;
  }

  function applyOverlayPages(
    pages: OverlayPage[],
    options: {
      mode?: "push" | "replace" | "history";
      historyDepth?: number;
    } = {},
  ) {
    const mode = options.mode ?? "replace";
    const sanitizedPages = sanitizeOverlayPages(pages);
    const nextHistoryDepth =
      options.historyDepth ??
      (mode === "push" ? overlayHistoryDepth + 1 : overlayHistoryDepth);

    overlayPages.value = cloneOverlayPages(sanitizedPages);
    overlayHistoryDepth = nextHistoryDepth;
    closeTransientChrome();

    if (!overlayNavigationReady || mode === "history" || typeof window === "undefined") {
      return;
    }

    const state = createOverlayHistoryState(sanitizedPages, nextHistoryDepth);
    const url = overlayNavigationUrl(sanitizedPages);

    if (mode === "push") {
      window.history.pushState(state, "", url);
      return;
    }

    window.history.replaceState(state, "", url);
  }

  function initializeOverlayNavigation() {
    if (typeof window === "undefined") {
      return;
    }

    const historyState = parseOverlayHistoryState(window.history.state);
    const historyPages = sanitizeOverlayPages(historyState?.overlayPages ?? []);
    const hashPages = sanitizeOverlayPages(parseOverlayRouteHash(window.location.hash));

    popstateHandler = (event: PopStateEvent) => {
      const state = parseOverlayHistoryState(event.state);
      const nextPages = sanitizeOverlayPages(
        state?.overlayPages ?? parseOverlayRouteHash(window.location.hash),
      );
      applyOverlayPages(nextPages, {
        mode: "history",
        historyDepth: state?.overlayDepth ?? 0,
      });
    };

    overlayNavigationReady = true;

    if (historyPages.length) {
      applyOverlayPages(historyPages, {
        historyDepth: historyState?.overlayDepth ?? historyPages.length,
      });
    } else if (hashPages.length) {
      window.history.replaceState(
        createOverlayHistoryState([], 0),
        "",
        overlayNavigationUrl([]),
      );
      overlayHistoryDepth = 0;
      applyOverlayPages(hashPages, { mode: "push" });
    } else {
      applyOverlayPages([], { historyDepth: 0 });
    }

    window.addEventListener("popstate", popstateHandler);
  }

  watch(
    visibleSessionsForCircle,
    (list) => {
      if (!list.some((session) => session.id === selectedSessionId.value)) {
        selectedSessionId.value = list[0]?.id ?? "";
      }
    },
    { immediate: true },
  );

  watch(
    () => selectedSession.value?.id,
    (sessionId, previousSessionId) => {
      if (sessionId === previousSessionId) {
        return;
      }

      composerText.value = selectedSession.value?.draft ?? "";
      void ensureSessionMessagesLoaded(sessionId);
    },
    { immediate: true },
  );

  watch(
    [isAuthenticated, circles, contacts, sessions, groups],
    () => {
      const nextPages = sanitizeOverlayPages(overlayPages.value);
      if (!overlayPagesMatch(overlayPages.value, nextPages)) {
        applyOverlayPages(nextPages);
      }
    },
    { deep: true },
  );

  watch(
    [
      isAuthenticated,
      authSession,
      userProfile,
      circles,
      appPreferences,
      notificationPreferences,
      advancedPreferences,
      activeCircleId,
      selectedSessionId,
      sessions,
      contacts,
      groups,
      messageStore,
    ],
    () => {
      if (!isShellStateReady.value) {
        return;
      }

      schedulePersistence();
    },
    { deep: true },
  );

  watch(
    [circles, sessions, contacts, groups, messageStore, activeCircleId, advancedPreferences],
    () => {
      if (transportTimer) {
        window.clearTimeout(transportTimer);
      }

      transportTimer = window.setTimeout(() => {
        transportTimer = null;
        void refreshTransportSnapshot();
      }, 180);
    },
    { deep: true },
  );

  watch(
    [isAuthenticated, advancedPreferences],
    () => {
      restartTransportHeartbeat();
    },
    { deep: true, immediate: true },
  );

  function resolvedShellTheme() {
    if (appPreferences.value.theme !== "system") {
      return appPreferences.value.theme;
    }

    if (typeof window !== "undefined" && typeof window.matchMedia === "function") {
      return window.matchMedia("(prefers-color-scheme: dark)").matches ? "ink" : "light";
    }

    return "light";
  }

  function syncSystemThemeListener() {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
      return;
    }

    if (systemThemeMediaQuery && systemThemeChangeHandler) {
      systemThemeMediaQuery.removeEventListener("change", systemThemeChangeHandler);
      systemThemeChangeHandler = null;
    }

    if (appPreferences.value.theme !== "system") {
      systemThemeMediaQuery = null;
      return;
    }

    systemThemeMediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    systemThemeChangeHandler = () => {
      applyShellAppearance();
    };
    systemThemeMediaQuery.addEventListener("change", systemThemeChangeHandler);
  }

  function resolvedShellLanguage() {
    if (appPreferences.value.language !== "system") {
      return appPreferences.value.language;
    }

    if (typeof navigator !== "undefined" && navigator.language) {
      return navigator.language;
    }

    return "en";
  }

  function applyShellAppearance() {
    if (typeof document === "undefined") {
      return;
    }

    const root = document.documentElement;
    root.dataset.shellTheme = resolvedShellTheme();
    root.dataset.shellThemePreference = appPreferences.value.theme;
    root.dataset.shellTextSize = appPreferences.value.textSize;
    root.dataset.shellLanguage = appPreferences.value.language;
    root.lang = resolvedShellLanguage();
  }

  watch(
    appPreferences,
    () => {
      syncSystemThemeListener();
      applyShellAppearance();
    },
    { deep: true, immediate: true },
  );

  onBeforeUnmount(() => {
    flushPendingPersistence("local");
    clearTransportNoticeTimer();

    if (persistTimer) {
      window.clearTimeout(persistTimer);
    }

    if (draftPersistTimer) {
      window.clearTimeout(draftPersistTimer);
    }

    if (transportTimer) {
      window.clearTimeout(transportTimer);
    }

    stopTransportHeartbeat();

    if (popstateHandler) {
      window.removeEventListener("popstate", popstateHandler);
      popstateHandler = null;
    }

    if (pagehideHandler) {
      window.removeEventListener("pagehide", pagehideHandler);
      pagehideHandler = null;
    }

    if (visibilitychangeHandler && typeof document !== "undefined") {
      document.removeEventListener("visibilitychange", visibilitychangeHandler);
      visibilitychangeHandler = null;
    }

    if (systemThemeMediaQuery && systemThemeChangeHandler) {
      systemThemeMediaQuery.removeEventListener("change", systemThemeChangeHandler);
      systemThemeChangeHandler = null;
      systemThemeMediaQuery = null;
    }

    overlayNavigationReady = false;
    overlayHistoryDepth = 0;
  });

  onMounted(async () => {
    try {
      bootstrapStatus.value = await invoke<BootstrapStatus>("bootstrap_status");
    } catch {
      bootstrapStatus.value = null;
    }

    const snapshot = await loadChatShellSnapshot(createChatSeedFallback());
    applyChatShellSnapshot(snapshot);
    initializeOverlayNavigation();
    pagehideHandler = () => {
      flushPendingPersistence("full");
    };
    visibilitychangeHandler = () => {
      if (document.hidden) {
        flushPendingPersistence("full");
      }
    };
    window.addEventListener("pagehide", pagehideHandler);
    document.addEventListener("visibilitychange", visibilitychangeHandler);
    isShellStateReady.value = true;
    await refreshTransportSnapshot();
    schedulePersistence();

    window.setTimeout(() => {
      showLaunch.value = false;
    }, 950);
  });

  function snapshotShellState(): PersistedShellState {
    return cloneState({
      isAuthenticated: isAuthenticated.value,
      authSession: authSession.value,
      userProfile: userProfile.value,
      circles: circles.value,
      appPreferences: appPreferences.value,
      notificationPreferences: notificationPreferences.value,
      advancedPreferences: advancedPreferences.value,
      activeCircleId: activeCircleId.value,
      selectedSessionId: selectedSessionId.value,
      sessions: sessions.value,
      contacts: contacts.value,
      groups: groups.value,
      messageStore: messageStore.value,
    }) as PersistedShellState;
  }

  function snapshotChatShellState(): ChatShellSnapshot {
    return createChatShellSnapshotFromPersistedState(snapshotShellState());
  }

  function clearDraftPersistenceTimer() {
    if (draftPersistTimer) {
      window.clearTimeout(draftPersistTimer);
      draftPersistTimer = null;
    }
  }

  async function persistSessionDraftToRuntime(
    sessionId: string,
    draft: string,
    mutationSerial: number,
  ) {
    const nextSeed = await updateChatSessionDraft({ sessionId, draft });
    if (!nextSeed || mutationSerial !== latestDraftMutationSerial) {
      return;
    }

    applyDomainSeed(nextSeed, {
      preferredCircleId: activeCircleId.value,
      preferredSessionId: selectedSessionId.value || sessionId,
    });
  }

  async function flushPendingDraftPersistence() {
    if (!pendingDraftSessionId) {
      return;
    }

    clearDraftPersistenceTimer();

    const sessionId = pendingDraftSessionId;
    const draft = pendingDraftValue;
    const mutationSerial = ++latestDraftMutationSerial;

    pendingDraftSessionId = null;
    pendingDraftValue = "";

    await persistSessionDraftToRuntime(sessionId, draft, mutationSerial);
  }

  function cancelPendingDraftPersistence() {
    latestDraftMutationSerial += 1;
    pendingDraftSessionId = null;
    pendingDraftValue = "";
    clearDraftPersistenceTimer();
  }

  function flushPendingPersistence(mode: "local" | "full" = "full") {
    if (!isShellStateReady.value) {
      return;
    }

    if (persistTimer) {
      window.clearTimeout(persistTimer);
      persistTimer = null;
    }

    void flushPendingDraftPersistence();

    const snapshot = snapshotChatShellState();
    persistChatShellSnapshotLocally(snapshot);

    if (mode === "full") {
      void saveChatShellSnapshot(snapshot);
    }
  }

  async function persistState() {
    await flushPendingDraftPersistence();
    await saveChatShellSnapshot(snapshotChatShellState());
  }

  function setTransportSnapshot(snapshot: TransportSnapshot, options: { suppressNotice?: boolean } = {}) {
    const previousSnapshot = transportSnapshot.value;
    transportSnapshot.value = snapshot;

    if (!options.suppressNotice) {
      maybeShowTransportNoticeFromSnapshot(previousSnapshot, snapshot);
    }

    restartTransportHeartbeat();
  }

  async function refreshTransportSnapshot(
    options: {
      pendingActivity?: {
        circleId: string;
        action: TransportCircleAction;
      };
      suppressNotice?: boolean;
    } = {},
  ) {
    const result = await loadTransportSnapshot(
      {
        activeCircleId: activeCircleId.value || undefined,
        useTorNetwork: advancedPreferences.value.useTorNetwork,
        experimentalTransport: advancedPreferences.value.experimentalTransport,
      },
      {
        circles: circles.value,
        contacts: contacts.value,
        sessions: sessions.value,
        groups: groups.value,
        messageStore: messageStore.value,
        activeCircleId: activeCircleId.value,
        advanced: advancedPreferences.value,
        previousSnapshot: transportSnapshot.value,
        pendingActivity: options.pendingActivity,
      },
    );
    setTransportSnapshot(result.snapshot, {
      suppressNotice: options.suppressNotice,
    });

    if (result.source === "tauri") {
      await refreshDomainOverview();
      await refreshSessionMessageUpdates(selectedSession.value?.id);
    }

    if (result.source !== "fallback") {
      return;
    }

    const recoveryAction = deriveRuntimeRecoveryAction(result.snapshot, circles.value);
    if (!recoveryAction || transportBusyCircleId.value) {
      return;
    }

    applyLocalTransportCircleAction(recoveryAction.circleId, recoveryAction.action);
  }

  function stopTransportHeartbeat() {
    if (transportHeartbeatTimer) {
      window.clearInterval(transportHeartbeatTimer);
      transportHeartbeatTimer = null;
    }

    activeTransportHeartbeatIntervalMs = 0;
  }

  function desiredTransportHeartbeatIntervalMs() {
    if (!isAuthenticated.value) {
      return 0;
    }

    if (
      !advancedPreferences.value.relayDiagnostics &&
      !advancedPreferences.value.experimentalTransport
    ) {
      return 0;
    }

    const runtimeRecoveryActive = !!transportSnapshot.value?.runtimeSessions.some((session) => {
      return session.queueState === "queued" || session.queueState === "backoff";
    });
    if (runtimeRecoveryActive) {
      return 3_000;
    }

    return advancedPreferences.value.relayDiagnostics ? 12_000 : 16_000;
  }

  function restartTransportHeartbeat() {
    const intervalMs = desiredTransportHeartbeatIntervalMs();
    if (intervalMs === 0) {
      stopTransportHeartbeat();
      return;
    }

    if (transportHeartbeatTimer && activeTransportHeartbeatIntervalMs === intervalMs) {
      return;
    }

    stopTransportHeartbeat();
    activeTransportHeartbeatIntervalMs = intervalMs;
    transportHeartbeatTimer = window.setInterval(() => {
      void refreshTransportSnapshot();
    }, intervalMs);
  }

  function applyTransportMutationResult(
    result: TransportMutationResult,
    options?: { preferredCircleId?: string; preferredSessionId?: string },
  ) {
    applyDomainSeed(result.seed, options);
    setTransportSnapshot(result.snapshot);
  }

  function schedulePersistence() {
    if (!isShellStateReady.value) {
      return;
    }

    if (persistTimer) {
      window.clearTimeout(persistTimer);
    }

    persistTimer = window.setTimeout(() => {
      persistTimer = null;
      void persistState();
    }, 240);
  }

  function buildSessionMessagePageState(messages: MessageItem[]): {
    messages: MessageItem[];
    page: SessionMessagePageState;
  } {
    const hasMore = messages.length > SESSION_MESSAGE_PAGE_SIZE;
    const trimmedMessages = hasMore ? messages.slice(-SESSION_MESSAGE_PAGE_SIZE) : messages;

    return {
      messages: trimmedMessages,
      page: {
        initialized: true,
        loading: false,
        hasMore,
        nextBeforeMessageId: hasMore ? trimmedMessages[0]?.id : undefined,
      },
    };
  }

  function buildLoadedSessionMessageState(): SessionMessagePageState {
    return {
      initialized: true,
      loading: false,
      hasMore: false,
    };
  }

  function mergeLoadedSessionMessages(existing: MessageItem[], incoming: MessageItem[]) {
    const incomingIndex = new Map(incoming.map((message) => [message.id, message]));
    const merged = existing.map((message) => incomingIndex.get(message.id) ?? message);
    const existingIds = new Set(existing.map((message) => message.id));

    for (const message of incoming) {
      if (!existingIds.has(message.id)) {
        merged.push(message);
      }
    }

    return merged;
  }

  function sessionSubtitleFromMessage(message: MessageItem) {
    return message.body;
  }

  function setSessionMessagePageState(sessionId: string, page: SessionMessagePageState) {
    sessionMessagePages.value = {
      ...sessionMessagePages.value,
      [sessionId]: page,
    };
  }

  function mergeSessionMessagePage(
    sessionId: string,
    messages: MessageItem[],
    page: SessionMessagePageState,
  ) {
    messageStore.value = {
      ...messageStore.value,
      [sessionId]: messages,
    };
    setSessionMessagePageState(sessionId, page);
  }

  async function ensureSessionMessagesLoaded(sessionId?: string, options: { force?: boolean } = {}) {
    if (!sessionId) {
      return;
    }

    const currentPage = sessionMessagePages.value[sessionId];
    if (currentPage?.loading) {
      return;
    }

    if (currentPage?.initialized && !options.force) {
      return;
    }

    setSessionMessagePageState(sessionId, {
      initialized: false,
      loading: true,
      hasMore: currentPage?.hasMore ?? false,
      nextBeforeMessageId: currentPage?.nextBeforeMessageId,
    });

    const page = await loadChatSessionMessages(
      {
        sessionId,
        limit: SESSION_MESSAGE_PAGE_SIZE,
      },
      messageStore.value,
    );

    mergeSessionMessagePage(sessionId, page.messages, {
      initialized: true,
      loading: false,
      hasMore: page.hasMore,
      nextBeforeMessageId: page.nextBeforeMessageId,
    });
  }

  async function refreshSessionMessageUpdates(sessionId?: string) {
    if (!sessionId) {
      return;
    }

    const currentPage = sessionMessagePages.value[sessionId];
    if (!currentPage?.initialized || currentPage.loading) {
      return;
    }

    const existingMessages = messageStore.value[sessionId] ?? [];
    let mergedMessages = existingMessages;
    let afterMessageId = existingMessages[existingMessages.length - 1]?.id;
    let hasMore = true;
    let changed = false;
    let safety = 0;

    while (hasMore && safety < 6) {
      const updates = await loadChatSessionMessageUpdates(
        {
          sessionId,
          afterMessageId,
          limit: SESSION_MESSAGE_PAGE_SIZE,
        },
        messageStore.value,
      );

      if (!updates.messages.length) {
        break;
      }

      mergedMessages = mergeLoadedSessionMessages(mergedMessages, updates.messages);
      afterMessageId = updates.messages[updates.messages.length - 1]?.id ?? afterMessageId;
      hasMore = updates.hasMore;
      changed = true;
      safety += 1;
    }

    if (!changed) {
      return;
    }

    mergeSessionMessagePage(sessionId, mergedMessages, {
      ...currentPage,
      initialized: true,
      loading: false,
    });

    const lastMessage = mergedMessages[mergedMessages.length - 1];
    if (lastMessage) {
      updateSession(sessionId, {
        subtitle: sessionSubtitleFromMessage(lastMessage),
        time: lastMessage.time,
        unreadCount: undefined,
      });
    }
  }

  async function loadOlderMessages() {
    const sessionId = selectedSession.value?.id;
    if (!sessionId) {
      return;
    }

    const currentPage = sessionMessagePages.value[sessionId];
    if (
      !currentPage?.initialized ||
      currentPage.loading ||
      !currentPage.hasMore ||
      !currentPage.nextBeforeMessageId
    ) {
      return;
    }

    setSessionMessagePageState(sessionId, {
      ...currentPage,
      loading: true,
    });

    const page = await loadChatSessionMessages(
      {
        sessionId,
        beforeMessageId: currentPage.nextBeforeMessageId,
        limit: SESSION_MESSAGE_PAGE_SIZE,
      },
      messageStore.value,
    );
    const existingMessages = messageStore.value[sessionId] ?? [];
    const existingIds = new Set(existingMessages.map((message) => message.id));
    const olderMessages = page.messages.filter((message) => !existingIds.has(message.id));

    mergeSessionMessagePage(sessionId, [...olderMessages, ...existingMessages], {
      initialized: true,
      loading: false,
      hasMore: page.hasMore,
      nextBeforeMessageId: page.nextBeforeMessageId,
    });
  }

  function applyDomainOverview(
    overview: ChatDomainOverview,
    options?: {
      preferredCircleId?: string;
      preferredSessionId?: string;
    },
  ) {
    const nextCircles = overview.circles.length ? overview.circles : initialShellState.circles;
    const nextCircleId = nextCircles.some((circle) => circle.id === options?.preferredCircleId)
      ? options?.preferredCircleId ?? ""
      : nextCircles.some((circle) => circle.id === activeCircleId.value)
        ? activeCircleId.value
        : nextCircles[0]?.id ?? "";

    const localSessionIndex = new Map(sessions.value.map((session) => [session.id, session]));
    const nextSessions = overview.sessions.map((session) => {
      const localSession = localSessionIndex.get(session.id);
      if (pendingDraftSessionId === session.id) {
        return {
          ...session,
          draft: pendingDraftValue || undefined,
        };
      }

      if (localSession?.draft !== undefined) {
        return {
          ...session,
          draft: localSession.draft,
        };
      }

      if (selectedSessionId.value === session.id && composerText.value) {
        return {
          ...session,
          draft: composerText.value,
        };
      }

      return session;
    });

    circles.value = nextCircles;
    activeCircleId.value = nextCircleId;
    sessions.value = nextSessions;
    contacts.value = overview.contacts;
    groups.value = overview.groups;
    const validSessionIds = new Set(nextSessions.map((session) => session.id));
    const nextMessageStore = Object.fromEntries(
      Object.entries(messageStore.value).filter(([sessionId]) => validSessionIds.has(sessionId)),
    ) as Record<string, MessageItem[]>;
    const nextSessionMessagePages = Object.fromEntries(
      Object.entries(sessionMessagePages.value).filter(([sessionId]) => validSessionIds.has(sessionId)),
    ) as Record<string, SessionMessagePageState>;

    messageStore.value = nextMessageStore;
    sessionMessagePages.value = nextSessionMessagePages;

    const visibleSessions = nextSessions.filter((session) => {
      return session.circleId === nextCircleId && !session.archived;
    });
    const nextSessionId = visibleSessions.some((session) => session.id === options?.preferredSessionId)
      ? options?.preferredSessionId ?? ""
      : visibleSessions.some((session) => session.id === selectedSessionId.value)
        ? selectedSessionId.value
        : visibleSessions[0]?.id ?? "";

    selectedSessionId.value = nextSessionId;
  }

  function applyDomainSeed(
    seed: ChatDomainSeed,
    options?: {
      preferredCircleId?: string;
      preferredSessionId?: string;
      messageStoreMode?: DomainSeedMessageMode;
    },
  ) {
    applyDomainOverview(
      {
        circles: seed.circles,
        contacts: seed.contacts,
        sessions: seed.sessions,
        groups: seed.groups,
      },
      options,
    );
    const messageStoreMode = options?.messageStoreMode ?? "full";
    const nextMessageStore = { ...messageStore.value };
    const nextSessionMessagePages = { ...sessionMessagePages.value };

    for (const [sessionId, messages] of Object.entries(seed.messageStore)) {
      if (messageStoreMode === "preview") {
        const normalized = buildSessionMessagePageState(messages);
        nextMessageStore[sessionId] = normalized.messages;
        nextSessionMessagePages[sessionId] = normalized.page;
        continue;
      }

      nextMessageStore[sessionId] = messages;
      nextSessionMessagePages[sessionId] = buildLoadedSessionMessageState();
    }

    messageStore.value = nextMessageStore;
    sessionMessagePages.value = nextSessionMessagePages;
  }

  function applyShellSnapshot(state: ShellStateSnapshot) {
    isAuthenticated.value = state.isAuthenticated;
    authSession.value = state.authSession;
    userProfile.value = state.userProfile;
    appPreferences.value = state.appPreferences;
    notificationPreferences.value = state.notificationPreferences;
    advancedPreferences.value = state.advancedPreferences;
  }

  function applyChatShellSnapshot(snapshot: ChatShellSnapshot) {
    applyShellSnapshot(snapshot.shell);
    applyDomainSeed(snapshot.domain, {
      preferredCircleId: snapshot.shell.activeCircleId,
      preferredSessionId: snapshot.shell.selectedSessionId,
      messageStoreMode: "preview",
    });
    if (selectedSessionId.value) {
      const currentPage = sessionMessagePages.value[selectedSessionId.value];
      setSessionMessagePageState(selectedSessionId.value, {
        initialized: false,
        loading: false,
        hasMore: currentPage?.hasMore ?? false,
        nextBeforeMessageId: currentPage?.nextBeforeMessageId,
      });
      void ensureSessionMessagesLoaded(selectedSessionId.value, { force: true });
    }
  }

  function updateSession(sessionId: string, patch: Partial<SessionItem>) {
    sessions.value = sessions.value.map((session) => {
      if (session.id !== sessionId) {
        return session;
      }

      return {
        ...session,
        ...patch,
      };
    });
  }

  function domainOverviewEqual(left: ChatDomainOverview, right: ChatDomainOverview) {
    return JSON.stringify(left) === JSON.stringify(right);
  }

  async function refreshDomainOverview() {
    const currentOverview: ChatDomainOverview = {
      circles: circles.value,
      contacts: contacts.value,
      sessions: sessions.value,
      groups: groups.value,
    };
    const nextOverview = await loadChatDomainOverview(currentOverview);
    if (domainOverviewEqual(nextOverview, currentOverview)) {
      return;
    }

    applyDomainOverview(nextOverview, {
      preferredCircleId: activeCircleId.value,
      preferredSessionId: selectedSessionId.value,
    });
  }

  function messageDeliveryStatusForCircle(circleId: string): MessageItem["deliveryStatus"] {
    const status = circles.value.find((circle) => circle.id === circleId)?.status;

    switch (status as CircleStatus | undefined) {
      case "open":
        return "sent";
      case "connecting":
        return "sending";
      case "closed":
      default:
        return "failed";
    }
  }

  function applyLocalSessionDraft(sessionId: string, draft: string) {
    updateSession(sessionId, {
      draft: draft ? draft : undefined,
    });
  }

  function applyLocalMessageDeliveryStatus(
    sessionId: string,
    messageId: string,
    deliveryStatus: MessageItem["deliveryStatus"],
  ) {
    messageStore.value = Object.fromEntries(
      Object.entries(messageStore.value).map(([currentSessionId, messages]) => {
        if (currentSessionId !== sessionId) {
          return [currentSessionId, messages];
        }

        return [
          currentSessionId,
          messages.map((message) => {
            if (message.id !== messageId) {
              return message;
            }

            return {
              ...message,
              deliveryStatus,
            };
          }),
        ];
      }),
    );
  }

  function scheduleDraftPersistence(sessionId: string, draft: string) {
    pendingDraftSessionId = sessionId;
    pendingDraftValue = draft;
    clearDraftPersistenceTimer();

    const mutationSerial = ++latestDraftMutationSerial;
    draftPersistTimer = window.setTimeout(() => {
      draftPersistTimer = null;
      pendingDraftSessionId = null;
      pendingDraftValue = "";
      void persistSessionDraftToRuntime(sessionId, draft, mutationSerial);
    }, 220);
  }

  function updateComposerText(value: string) {
    composerText.value = value;

    if (!selectedSession.value) {
      return;
    }

    applyLocalSessionDraft(selectedSession.value.id, value);
    scheduleDraftPersistence(selectedSession.value.id, value);
  }

  function selectSession(sessionId: string) {
    selectedSessionId.value = sessionId;
    sessions.value = sessions.value.map((session) => {
      if (session.id === sessionId) {
        return {
          ...session,
          unreadCount: undefined,
        };
      }

      return session;
    });
  }

  function chooseCircle(circleId: string) {
    if (!circles.value.some((circle) => circle.id === circleId)) {
      return;
    }

    activeCircleId.value = circleId;
    searchText.value = "";
    closeTransientChrome();
    closeAllOverlayPages();
  }

  function toggleCircleSwitcher() {
    showCircleSwitcher.value = !showCircleSwitcher.value;
  }

  function openNewMessage() {
    pushOverlayPage({ kind: "new-message" });
  }

  function openFindPeoplePage(mode: "chat" | "join-circle" = "chat") {
    pushOverlayPage({ kind: "find-people", mode });
  }

  function openCircleManagement() {
    pushOverlayPage({ kind: "circle-directory" });
  }

  function openCircleDetail(circleId: string) {
    if (!circles.value.some((circle) => circle.id === circleId)) {
      return;
    }

    pushOverlayPage({ kind: "circle-detail", circleId });
  }

  function openDetailsDrawer() {
    if (!selectedSession.value) {
      return;
    }

    showDetailsDrawer.value = true;
  }

  function openContactProfile(contactId: string) {
    if (!contacts.value.some((contact) => contact.id === contactId)) {
      return;
    }

    pushOverlayPage({ kind: "contact", contactId });
  }

  function openGroupProfilePage(sessionId: string) {
    if (
      !sessions.value.some((session) => session.id === sessionId) ||
      !groups.value.some((group) => group.sessionId === sessionId)
    ) {
      return;
    }

    pushOverlayPage({ kind: "group", sessionId });
  }

  function openGroupSelectMembersPage() {
    pushOverlayPage({ kind: "group-select-members" });
  }

  function openGroupCreatePage(memberContactIds: string[]) {
    const nextMemberContactIds = Array.from(new Set(memberContactIds.filter(Boolean))).filter((contactId) => {
      return contacts.value.some((contact) => contact.id === contactId);
    });
    if (!nextMemberContactIds.length) {
      return;
    }

    pushOverlayPage({
      kind: "group-create",
      memberContactIds: nextMemberContactIds,
    });
  }

  function openGroupNamePage(sessionId: string) {
    if (!groups.value.some((group) => group.sessionId === sessionId)) {
      return;
    }

    pushOverlayPage({ kind: "group-name", sessionId });
  }

  function openGroupMembersPage(sessionId: string) {
    if (!groups.value.some((group) => group.sessionId === sessionId)) {
      return;
    }

    pushOverlayPage({ kind: "group-members", sessionId });
  }

  function openGroupAddMembersPage(sessionId: string) {
    if (!groups.value.some((group) => group.sessionId === sessionId)) {
      return;
    }

    pushOverlayPage({ kind: "group-add-members", sessionId });
  }

  function openGroupRemoveMembersPage(sessionId: string) {
    if (!groups.value.some((group) => group.sessionId === sessionId)) {
      return;
    }

    pushOverlayPage({ kind: "group-remove-members", sessionId });
  }

  function openProfilePage() {
    if (!selectedSession.value) {
      return;
    }

    if (selectedSession.value.kind === "group") {
      openGroupProfilePage(selectedSession.value.id);
      return;
    }

    if (selectedContact.value) {
      openContactProfile(selectedContact.value.id);
      return;
    }

    showDetailsDrawer.value = true;
  }

  function handleSettingsAction(actionId: SettingPageId) {
    pushOverlayPage({ kind: "settings-detail", settingId: actionId });
  }

  function closeCircleOverlay() {
    showCircleSwitcher.value = false;
  }

  function buildMaskedAccessLabel(value: string, options: { prefix?: number; suffix?: number } = {}) {
    const trimmed = value.trim();
    if (!trimmed) {
      return "";
    }

    const prefix = options.prefix ?? 6;
    const suffix = options.suffix ?? 4;
    if (trimmed.length <= prefix + suffix) {
      return trimmed;
    }

    return `${trimmed.slice(0, prefix)}...${trimmed.slice(-suffix)}`;
  }

  function buildAuthSessionSummary(input: LoginCompletionInput): AuthSessionSummary {
    const accessLabel =
      input.method === "quickStart"
        ? "Quick Start"
        : input.access.kind === "nostrConnect"
          ? buildMaskedAccessLabel(input.access.value ?? "", {
              prefix: 14,
              suffix: 6,
            }) || "nostrconnect://"
          : input.access.kind === "bunker"
            ? buildMaskedAccessLabel(input.access.value ?? "", {
                prefix: 10,
                suffix: 6,
              }) || "bunker://"
            : buildMaskedAccessLabel(input.access.value ?? "", {
                prefix: 8,
                suffix: 4,
              }) || input.access.kind;

    return {
      loginMethod: input.method,
      access: {
        kind: input.access.kind,
        label: accessLabel,
      },
      circleSelectionMode: input.circleSelection.mode,
      loggedInAt: new Date().toISOString(),
    };
  }

  function buildLoggedOutShellSnapshot(): ChatShellSnapshot {
    const loggedOutState = createLoggedOutShellState({
      appPreferences: cloneState(appPreferences.value),
      notificationPreferences: cloneState(notificationPreferences.value),
      advancedPreferences: cloneState(advancedPreferences.value),
    });

    return createChatShellSnapshotFromPersistedState(loggedOutState);
  }

  async function resolveLoginCircle(selection: LoginCircleSelectionInput) {
    if (selection.mode === "restore") {
      return circles.value[0]?.id ?? activeCircleId.value;
    }

    if (selection.mode === "existing") {
      return selection.circleId ?? circles.value[0]?.id ?? "";
    }

    const addInput: AddCircleInput =
      selection.mode === "custom"
        ? {
            mode: "custom",
            name: selection.name?.trim() || "Custom Relay",
            relay: selection.relay?.trim() || "",
          }
        : {
            mode: "invite",
            name: selection.name?.trim() || "Invite Circle",
            inviteCode: selection.inviteCode?.trim() || "",
          };

    const result = await addChatCircle(addInput);
    if (result) {
      applyDomainSeed(result.seed, {
        preferredCircleId: result.circleId,
        preferredSessionId: selectedSessionId.value,
      });
      return result.circleId;
    }

    return applyLocalAddCircleFromDirectory(addInput);
  }

  async function completeLogin(input: LoginCompletionInput) {
    isAuthenticated.value = true;
    authSession.value = buildAuthSessionSummary(input);
    userProfile.value = cloneState(input.userProfile) as UserProfile;
    closeTransientChrome();
    closeAllOverlayPages();
    await refreshDomainOverview();
    const nextCircleId = await resolveLoginCircle(input.circleSelection);

    if (nextCircleId) {
      chooseCircle(nextCircleId);
      return;
    }

    if (circles.value[0]?.id) {
      chooseCircle(circles.value[0].id);
    }
  }

  function logout() {
    cancelPendingDraftPersistence();
    if (persistTimer) {
      window.clearTimeout(persistTimer);
      persistTimer = null;
    }

    dismissTransportNotice();
    transportBusyCircleId.value = "";
    transportSnapshot.value = null;
    closeTransientChrome();
    closeAllOverlayPages();
    applyChatShellSnapshot(buildLoggedOutShellSnapshot());
    composerText.value = "";
    searchText.value = "";
    const snapshot = snapshotChatShellState();
    persistChatShellSnapshotLocally(snapshot);
    void saveChatShellSnapshot(snapshot);
  }

  function deleteSession(sessionId: string) {
    sessions.value = sessions.value.filter((session) => session.id !== sessionId);
    delete messageStore.value[sessionId];
    delete sessionMessagePages.value[sessionId];
  }

  function buildUniqueSessionId(baseId: string) {
    let candidate = baseId;
    let suffix = 2;

    while (sessions.value.some((session) => session.id === candidate)) {
      candidate = `${baseId}-${suffix}`;
      suffix += 1;
    }

    return candidate;
  }

  function buildInitials(value: string) {
    const words = value
      .trim()
      .split(/[^a-zA-Z0-9]+/)
      .filter(Boolean);

    if (!words.length) {
      return "XC";
    }

    if (words.length === 1) {
      return words[0].slice(0, 2).toUpperCase();
    }

    return words
      .slice(0, 2)
      .map((word) => word.charAt(0))
      .join("")
      .toUpperCase();
  }

  function dedupeContactIds(contactIds: string[]) {
    return Array.from(
      new Set(contactIds.map((contactId) => contactId.trim()).filter(Boolean)),
    );
  }

  function humanizeLabel(value: string) {
    return value
      .trim()
      .split(/[^a-zA-Z0-9]+/)
      .filter(Boolean)
      .map((token) => token.charAt(0).toUpperCase() + token.slice(1).toLowerCase())
      .join(" ");
  }

  function buildSuggestedGroupName(memberContacts: ContactItem[]) {
    const ownerName = userProfile.value.name.trim() || "My";
    if (memberContacts.length === 1) {
      return `${memberContacts[0].name} & ${ownerName}`;
    }

    const possessiveOwner = ownerName.endsWith("s") ? `${ownerName}'` : `${ownerName}'s`;
    return `${possessiveOwner} Group`;
  }

  function inferCircleInputFromQuery(query: string): AddCircleInput {
    const trimmed = query.trim();
    const isRelayUrl =
      /^wss?:\/\//i.test(trimmed) ||
      /^mesh:\/\//i.test(trimmed) ||
      /^invite:\/\//i.test(trimmed);

    if (isRelayUrl && !/^invite:\/\//i.test(trimmed)) {
      let relayLabel = "Custom Relay";
      try {
        const url = new URL(trimmed);
        relayLabel = humanizeLabel(url.hostname.split(".")[0] ?? url.hostname) || relayLabel;
      } catch {
        relayLabel = humanizeLabel(trimmed.replace(/^[a-z]+:\/\//i, "")) || relayLabel;
      }

      return {
        mode: "custom",
        name: relayLabel,
        relay: trimmed,
      };
    }

    return {
      mode: "invite",
      name: "Invite Circle",
      inviteCode: trimmed,
    };
  }

  function applyLocalSendPreviewMessage(content: string, sessionId: string) {
    const session = sessions.value.find((item) => item.id === sessionId);
    const message: MessageItem = {
      id: `local-${Date.now()}`,
      kind: "text",
      author: "me",
      body: content,
      time: "now",
      deliveryStatus: messageDeliveryStatusForCircle(session?.circleId ?? activeCircleId.value),
    };

    messageStore.value[sessionId] = [...(messageStore.value[sessionId] ?? []), message];

    const targetIndex = sessions.value.findIndex((session) => session.id === sessionId);
    if (targetIndex < 0) {
      return;
    }

    const updatedSession = {
      ...sessions.value[targetIndex],
      subtitle: content,
      time: "now",
      draft: undefined,
    };

    const nextSessions = [...sessions.value];
    nextSessions.splice(targetIndex, 1);
    nextSessions.unshift(updatedSession);
    sessions.value = nextSessions;
  }

  async function sendPreviewMessage() {
    const content = composerText.value.trim();
    if (!content || !selectedSession.value) {
      return;
    }

    const sessionId = selectedSession.value.id;
    cancelPendingDraftPersistence();
    composerText.value = "";
    applyLocalSessionDraft(sessionId, "");
    const nextSeed = await sendChatMessage({ sessionId, body: content });

    if (nextSeed) {
      applyDomainSeed(nextSeed, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: sessionId,
      });
      return;
    }

    applyLocalSendPreviewMessage(content, sessionId);
  }

  async function retryMessageDelivery(messageId: string) {
    if (!selectedSession.value) {
      return;
    }

    const sessionId = selectedSession.value.id;
    const nextSeed = await retryChatMessageDelivery({ sessionId, messageId });
    if (nextSeed) {
      applyDomainSeed(nextSeed, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: sessionId,
      });
      return;
    }

    applyLocalMessageDeliveryStatus(
      sessionId,
      messageId,
      messageDeliveryStatusForCircle(selectedSession.value.circleId),
    );
  }

  function applyLocalStartConversation(contactId: string) {
    const existing = sessions.value.find((session) => {
      return session.circleId === activeCircleId.value && session.contactId === contactId;
    });

    if (existing) {
      if (existing.archived) {
        updateSession(existing.id, { archived: false });
      }

      selectSession(existing.id);
      return existing.id;
    }

    const contact = contacts.value.find((item) => item.id === contactId);
    if (!contact) {
      return null;
    }

    const sessionId = buildUniqueSessionId(`session-${contact.id}`);
    const newSession: SessionItem = {
      id: sessionId,
      circleId: activeCircleId.value,
      contactId: contact.id,
      name: contact.name,
      initials: contact.initials,
      subtitle: "Start a conversation",
      time: "now",
      kind: "direct",
      category: "friends",
    };

    sessions.value = [newSession, ...sessions.value];
    messageStore.value[sessionId] = [
      {
        id: `${sessionId}-system`,
        kind: "system",
        author: "system",
        body: `New conversation with ${contact.name}`,
        time: "",
      },
    ];
    selectedSessionId.value = sessionId;

    return sessionId;
  }

  function applyLocalStartSelfConversation() {
    const existing = sessions.value.find((session) => {
      return session.circleId === activeCircleId.value && session.kind === "self";
    });

    if (existing) {
      if (existing.archived) {
        updateSession(existing.id, { archived: false });
      }

      selectSession(existing.id);
      return existing.id;
    }

    const sessionId = buildUniqueSessionId(`self-${activeCircleId.value}`);
    const newSession: SessionItem = {
      id: sessionId,
      circleId: activeCircleId.value,
      name: "Note to Self",
      initials: "ME",
      subtitle: "Private note space",
      time: "now",
      kind: "self",
      category: "system",
    };

    sessions.value = [newSession, ...sessions.value];
    messageStore.value[sessionId] = [
      {
        id: `${sessionId}-system`,
        kind: "system",
        author: "system",
        body: "Private note space opened",
        time: "",
      },
    ];
    selectedSessionId.value = sessionId;

    return sessionId;
  }

  function applyLocalCreateLookupContact(query: string) {
    const normalized = query.trim();
    const existing = contacts.value.find((contact) => {
      return (
        contact.id.toLowerCase() === normalized.toLowerCase() ||
        contact.handle.toLowerCase() === normalized.toLowerCase() ||
        contact.pubkey.toLowerCase() === normalized.toLowerCase() ||
        contact.name.toLowerCase() === normalized.toLowerCase()
      );
    });

    if (existing) {
      return existing;
    }

    const slug = buildCircleSlug(normalized || "lookup");
    const inferredName = normalized.startsWith("@")
      ? normalized.slice(1)
      : normalized.includes("://")
        ? normalized.split("://").slice(-1)[0] ?? normalized
        : normalized;
    const contact: ContactItem = {
      id: `lookup-${slug}-${Date.now()}`,
      name: inferredName
        .split(/[^a-zA-Z0-9]+/)
        .filter(Boolean)
        .map((token) => token.charAt(0).toUpperCase() + token.slice(1).toLowerCase())
        .join(" ") || "Remote Contact",
      initials: buildInitials(inferredName),
      handle: normalized.startsWith("@") ? normalized.toLowerCase() : `@${slug}`,
      pubkey:
        normalized.startsWith("npub") || /^[a-f0-9]{32,}$/i.test(normalized)
          ? normalized
          : `lookup:${slug}`,
      subtitle: "Imported from lookup",
      bio: `Created locally from lookup query \`${normalized}\`.`,
      online: false,
      blocked: false,
    };

    contacts.value = [contact, ...contacts.value];
    return contact;
  }

  function applyLocalCreateGroupConversation(input: CreateGroupConversationInput) {
    const memberIds = dedupeContactIds(input.memberContactIds);
    if (!memberIds.length) {
      return null;
    }

    const memberContacts = memberIds
      .map((contactId) => contacts.value.find((contact) => contact.id === contactId))
      .filter((contact): contact is ContactItem => !!contact);

    if (!memberContacts.length) {
      return null;
    }

    const groupName =
      input.name.trim() || buildSuggestedGroupName(memberContacts);
    const sessionId = buildUniqueSessionId(`group-${buildCircleSlug(groupName)}`);
    sessions.value = [
      {
        id: sessionId,
        circleId: input.circleId,
        name: groupName,
        initials: buildInitials(groupName),
        subtitle: `Group created with ${memberContacts.length} contact${memberContacts.length > 1 ? "s" : ""}`,
        time: "now",
        kind: "group",
        category: "groups",
        members: memberContacts.length + 1,
      },
      ...sessions.value,
    ];
    groups.value = [
      {
        sessionId,
        name: groupName,
        description: `Group created from the new message flow in ${groupName}.`,
        members: memberContacts.map((contact, index) => ({
          contactId: contact.id,
          role: index === 0 ? "admin" : "member",
        })),
      },
      ...groups.value,
    ];
    messageStore.value[sessionId] = [
      {
        id: `${sessionId}-system`,
        kind: "system",
        author: "system",
        body: `Group created with ${memberContacts.map((contact) => contact.name).join(", ")}`,
        time: "",
      },
    ];
    selectedSessionId.value = sessionId;

    return sessionId;
  }

  function applyLocalUpdateGroupName(payload: UpdateGroupNameInput) {
    const nextName = payload.name.trim();
    if (!nextName) {
      return;
    }

    const currentGroup = groups.value.find((group) => group.sessionId === payload.sessionId);
    const currentSession = sessions.value.find((session) => session.id === payload.sessionId);
    if (!currentGroup || !currentSession) {
      return;
    }

    const previousDescription = currentGroup.description;
    const nextDescription =
      previousDescription === `Group created from the new message flow in ${currentGroup.name}.`
        ? `Group created from the new message flow in ${nextName}.`
        : previousDescription;

    groups.value = groups.value.map((group) => {
      if (group.sessionId !== payload.sessionId) {
        return group;
      }

      return {
        ...group,
        name: nextName,
        description: nextDescription,
      };
    });

    sessions.value = sessions.value.map((session) => {
      if (session.id !== payload.sessionId) {
        return session;
      }

      return {
        ...session,
        name: nextName,
        initials: buildInitials(nextName),
        subtitle:
          session.subtitle === previousDescription ? nextDescription : session.subtitle,
      };
    });
  }

  function applyLocalUpdateGroupMembers(payload: UpdateGroupMembersInput) {
    const memberContactIds = Array.from(
      new Set(payload.memberContactIds.map((contactId) => contactId.trim()).filter(Boolean)),
    );
    if (!memberContactIds.length) {
      return;
    }

    const currentGroup = groups.value.find((group) => group.sessionId === payload.sessionId);
    if (!currentGroup) {
      return;
    }

    const currentRoles = new Map(
      currentGroup.members.map((member) => [member.contactId, member.role ?? "member"]),
    );
    const nextMembers = memberContactIds.map((contactId) => ({
      contactId,
      role: currentRoles.get(contactId) ?? "member",
    }));

    if (!nextMembers.some((member) => member.role === "admin") && nextMembers[0]) {
      nextMembers[0] = {
        ...nextMembers[0],
        role: "admin",
      };
    }

    groups.value = groups.value.map((group) => {
      if (group.sessionId !== payload.sessionId) {
        return group;
      }

      return {
        ...group,
        members: nextMembers,
      };
    });

    sessions.value = sessions.value.map((session) => {
      if (session.id !== payload.sessionId) {
        return session;
      }

      return {
        ...session,
        members: nextMembers.length + 1,
      };
    });
  }

  async function startConversation(contactId: string) {
    if (!activeCircleId.value) {
      return;
    }

    const result = await startDirectConversation({
      circleId: activeCircleId.value,
      contactId,
    });

    if (result) {
      applyDomainSeed(result.seed, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: result.sessionId,
      });
      closeAllOverlayPages();
      return;
    }

    const sessionId = applyLocalStartConversation(contactId);
    if (sessionId) {
      closeAllOverlayPages();
    }
  }

  async function startSelfChat() {
    if (!activeCircleId.value) {
      return;
    }

    const result = await startSelfConversation({
      circleId: activeCircleId.value,
    });

    if (result) {
      applyDomainSeed(result.seed, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: result.sessionId,
      });
      closeAllOverlayPages();
      return;
    }

    const sessionId = applyLocalStartSelfConversation();
    if (sessionId) {
      closeAllOverlayPages();
    }
  }

  async function createGroupChat(input: Omit<CreateGroupConversationInput, "circleId">) {
    if (!activeCircleId.value) {
      return;
    }

    const result = await createGroupConversation({
      circleId: activeCircleId.value,
      name: input.name,
      memberContactIds: input.memberContactIds,
    });

    if (result) {
      applyDomainSeed(result.seed, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: result.sessionId,
      });
      closeAllOverlayPages();
      return;
    }

    const sessionId = applyLocalCreateGroupConversation({
      circleId: activeCircleId.value,
      name: input.name,
      memberContactIds: input.memberContactIds,
    });
    if (sessionId) {
      closeAllOverlayPages();
    }
  }

  async function joinCircleFromLookup(query: string) {
    const normalizedQuery = query.trim();
    if (!normalizedQuery) {
      return;
    }

    const input = inferCircleInputFromQuery(normalizedQuery);
    const result = await addChatCircle(input);
    if (result) {
      applyDomainSeed(result.seed, {
        preferredCircleId: result.circleId,
        preferredSessionId: selectedSessionId.value,
      });
      chooseCircle(result.circleId);
      return;
    }

    const circleId = applyLocalAddCircleFromDirectory(input);
    if (circleId) {
      chooseCircle(circleId);
    }
  }

  async function updateGroupName(payload: UpdateGroupNameInput) {
    const nextName = payload.name.trim();
    if (!nextName) {
      return;
    }

    const nextSeed = await updateChatGroupName({
      sessionId: payload.sessionId,
      name: nextName,
    });

    if (nextSeed) {
      applyDomainSeed(nextSeed, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: payload.sessionId,
      });
      closeTopOverlayPage();
      return;
    }

    applyLocalUpdateGroupName({
      sessionId: payload.sessionId,
      name: nextName,
    });
    closeTopOverlayPage();
  }

  async function updateGroupMembers(payload: UpdateGroupMembersInput) {
    const memberContactIds = Array.from(
      new Set(payload.memberContactIds.map((contactId) => contactId.trim()).filter(Boolean)),
    );
    if (!memberContactIds.length) {
      return;
    }

    const nextSeed = await updateChatGroupMembers({
      sessionId: payload.sessionId,
      memberContactIds,
    });

    if (nextSeed) {
      applyDomainSeed(nextSeed, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: payload.sessionId,
      });
      closeTopOverlayPage();
      return;
    }

    applyLocalUpdateGroupMembers({
      sessionId: payload.sessionId,
      memberContactIds,
    });
    closeTopOverlayPage();
  }

  async function startLookupChat(query: string) {
    if (!activeCircleId.value) {
      return;
    }

    const normalizedQuery = query.trim();
    if (!normalizedQuery) {
      return;
    }

    const result = await startLookupConversation({
      circleId: activeCircleId.value,
      query: normalizedQuery,
    });

    if (result) {
      applyDomainSeed(result.seed, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: result.sessionId,
      });
      closeAllOverlayPages();
      return;
    }

    const contact = applyLocalCreateLookupContact(normalizedQuery);
    const sessionId = applyLocalStartConversation(contact.id);
    if (sessionId) {
      closeAllOverlayPages();
    }
  }

  function applyLocalSessionAction(payload: { sessionId: string; action: SessionAction }) {
    const target = sessions.value.find((session) => session.id === payload.sessionId);
    if (!target) {
      return;
    }

    switch (payload.action) {
      case "pin":
        updateSession(payload.sessionId, { pinned: !target.pinned });
        break;
      case "mute":
        updateSession(payload.sessionId, { muted: !target.muted });
        if (target.kind === "group") {
          groups.value = groups.value.map((group) => {
            if (group.sessionId !== payload.sessionId) {
              return group;
            }

            return {
              ...group,
              muted: !group.muted,
            };
          });
        }
        break;
      case "archive":
        updateSession(payload.sessionId, { archived: true, pinned: false });
        if (selectedSessionId.value === payload.sessionId) {
          selectedSessionId.value = visibleSessionsForCircle.value[0]?.id ?? "";
        }
        break;
      case "unarchive":
        updateSession(payload.sessionId, { archived: false });
        selectSession(payload.sessionId);
        break;
      case "delete":
        deleteSession(payload.sessionId);
        break;
    }
  }

  async function handleSessionAction(payload: { sessionId: string; action: SessionAction }) {
    const nextSeed = await applyChatSessionAction(payload);
    if (nextSeed) {
      applyDomainSeed(nextSeed, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: payload.action === "unarchive" ? payload.sessionId : selectedSessionId.value,
      });
      return;
    }

    applyLocalSessionAction(payload);
  }

  async function openArchivedSession(sessionId: string) {
    await handleSessionAction({ sessionId, action: "unarchive" });
    closeTopOverlayPage();
  }

  function applyLocalToggleContactBlock(contactId: string) {
    contacts.value = contacts.value.map((contact) => {
      if (contact.id !== contactId) {
        return contact;
      }

      return {
        ...contact,
        blocked: !contact.blocked,
      };
    });
  }

  async function toggleContactBlock(contactId: string) {
    const nextSeed = await toggleChatContactBlock(contactId);
    if (nextSeed) {
      applyDomainSeed(nextSeed, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: selectedSessionId.value,
      });
      return;
    }

    applyLocalToggleContactBlock(contactId);
  }

  function toggleGroupMute(sessionId: string) {
    void handleSessionAction({ sessionId, action: "mute" });
  }

  async function leaveGroup(sessionId: string) {
    await handleSessionAction({ sessionId, action: "delete" });
    showDetailsDrawer.value = false;
    closeAllOverlayPages();
  }

  function openMemberProfile(contactId: string) {
    openContactProfile(contactId);
  }

  async function sendMessageFromProfile(contactId: string) {
    await startConversation(contactId);
    closeAllOverlayPages();
  }

  function openArchivedPage() {
    pushOverlayPage({ kind: "archived" });
  }

  function buildCircleSlug(value: string) {
    return value
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "circle";
  }

  function buildUniqueCircleId(baseLabel: string) {
    const baseId = buildCircleSlug(baseLabel);
    let candidate = baseId;
    let suffix = 2;

    while (circles.value.some((circle) => circle.id === candidate)) {
      candidate = `${baseId}-${suffix}`;
      suffix += 1;
    }

    return candidate;
  }

  function applyLocalAddCircleFromDirectory(payload: AddCircleInput) {
    const normalizedName =
      payload.name.trim() ||
      (payload.mode === "private"
        ? "Private Circle"
        : payload.mode === "custom"
          ? "Custom Relay"
          : "Invite Circle");

    const normalizedRelay =
      payload.mode === "private"
        ? `wss://${buildCircleSlug(normalizedName)}.private.circle.local`
        : payload.mode === "custom"
          ? payload.relay?.trim() ?? ""
          : payload.inviteCode?.trim().includes("://")
            ? payload.inviteCode.trim()
            : `invite://${payload.inviteCode?.trim() ?? ""}`;

    const existing = circles.value.find((circle) => {
      return circle.relay.trim().toLowerCase() === normalizedRelay.toLowerCase();
    });

    if (existing) {
      return existing.id;
    }

    const nextCircle = {
      id: buildUniqueCircleId(normalizedName),
      name: normalizedName,
      relay: normalizedRelay,
      type: payload.mode === "private" ? "paid" : payload.mode === "custom" ? "custom" : "default",
      status: "connecting",
      latency: "--",
      description:
        payload.mode === "private"
          ? "Private relay shell created from the onboarding flow."
          : payload.mode === "custom"
            ? "Custom relay connected from a manually entered endpoint."
            : "Circle imported from an invite handoff and waiting for relay confirmation.",
    } as const;

    circles.value = [nextCircle, ...circles.value];

    return nextCircle.id;
  }

  async function addCircleFromDirectory(payload: AddCircleInput) {
    const result = await addChatCircle(payload);
    if (result) {
      applyDomainSeed(result.seed, {
        preferredCircleId: result.circleId,
        preferredSessionId: selectedSessionId.value,
      });
      chooseCircle(result.circleId);
      return;
    }

    const circleId = applyLocalAddCircleFromDirectory(payload);
    if (circleId) {
      chooseCircle(circleId);
    }
  }

  function applyLocalUpdateCircle(payload: UpdateCircleInput) {
    const { circleId, name, description } = payload;
    const targetCircle = circles.value.find((circle) => circle.id === circleId);
    if (!targetCircle) {
      return;
    }

    const nextName = name.trim() || targetCircle.name || "Circle";
    const nextDescription = description.trim() || targetCircle.description;

    circles.value = circles.value.map((circle) => {
      if (circle.id !== circleId) {
        return circle;
      }

      return {
        ...circle,
        name: nextName,
        description: nextDescription,
      };
    });
  }

  async function updateCircle(payload: UpdateCircleInput) {
    const nextSeed = await updateChatCircle(payload);
    if (nextSeed) {
      applyDomainSeed(nextSeed, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: selectedSessionId.value,
      });
      return;
    }

    applyLocalUpdateCircle(payload);
  }

  function applyLocalRemoveCircle(circleId: string) {
    if (!circles.value.some((circle) => circle.id === circleId) || circles.value.length <= 1) {
      return;
    }

    const removedSessionIds = new Set(
      sessions.value
        .filter((session) => session.circleId === circleId)
        .map((session) => session.id),
    );

    const remainingCircles = circles.value.filter((circle) => circle.id !== circleId);
    const removedActiveCircle = activeCircleId.value === circleId;

    circles.value = remainingCircles;
    sessions.value = sessions.value.filter((session) => session.circleId !== circleId);
    groups.value = groups.value.filter((group) => !removedSessionIds.has(group.sessionId));
    messageStore.value = Object.fromEntries(
      Object.entries(messageStore.value).filter(([sessionId]) => !removedSessionIds.has(sessionId)),
    );
    sessionMessagePages.value = Object.fromEntries(
      Object.entries(sessionMessagePages.value).filter(
        ([sessionId]) => !removedSessionIds.has(sessionId),
      ),
    );

    if (removedSessionIds.has(selectedSessionId.value)) {
      selectedSessionId.value = "";
    }

    if (removedActiveCircle) {
      activeCircleId.value = remainingCircles[0]?.id ?? "";
      closeTransientChrome();
      closeAllOverlayPages();
      return;
    }

    applyOverlayPages(overlayPages.value.filter((page) => {
      if (page.kind === "circle-detail") {
        return page.circleId !== circleId;
      }

      const groupSessionId = overlayGroupSessionId(page);
      if (groupSessionId) {
        return !removedSessionIds.has(groupSessionId);
      }

      return true;
    }));
  }

  async function removeCircle(circleId: string) {
    if (!circles.value.some((circle) => circle.id === circleId) || circles.value.length <= 1) {
      return;
    }

    const removedSessionIds = new Set(
      sessions.value
        .filter((session) => session.circleId === circleId)
        .map((session) => session.id),
    );
    const removedActiveCircle = activeCircleId.value === circleId;
    const nextSeed = await removeChatCircle(circleId);

    if (nextSeed) {
      applyDomainSeed(nextSeed, {
        preferredCircleId: removedActiveCircle ? nextSeed.circles[0]?.id : activeCircleId.value,
        preferredSessionId: selectedSessionId.value,
      });

      if (removedActiveCircle) {
        closeTransientChrome();
        closeAllOverlayPages();
        return;
      }

      applyOverlayPages(overlayPages.value.filter((page) => {
        if (page.kind === "circle-detail") {
          return page.circleId !== circleId;
        }

        const groupSessionId = overlayGroupSessionId(page);
        if (groupSessionId) {
          return !removedSessionIds.has(groupSessionId);
        }

        return true;
      }));
      return;
    }

    applyLocalRemoveCircle(circleId);
  }

  function buildLocalTransportLatency(relay: string, type: string) {
    const protocolBase = advancedPreferences.value.experimentalTransport
      ? relay.startsWith("mesh://")
        ? 12
        : relay.startsWith("invite://")
          ? 48
          : 24
      : relay.startsWith("mesh://")
        ? 18
        : relay.startsWith("invite://")
          ? 72
          : 42;
    const circlePenalty = advancedPreferences.value.experimentalTransport
      ? type === "paid"
        ? 14
        : type === "bitchat"
          ? 4
          : type === "custom"
            ? 6
            : 0
      : type === "paid"
        ? 14
        : type === "bitchat"
          ? 6
          : type === "custom"
            ? 8
            : 0;
    const torPenalty = advancedPreferences.value.useTorNetwork
      ? advancedPreferences.value.experimentalTransport
        ? 20
        : 32
      : 0;
    const experimentalPenalty = advancedPreferences.value.experimentalTransport ? 4 : 0;

    return `${protocolBase + circlePenalty + torPenalty + experimentalPenalty} ms`;
  }

  function applyLocalTransportCircleAction(circleId: string, action: TransportCircleAction) {
    const targetCircle = circles.value.find((circle) => circle.id === circleId);
    if (!targetCircle) {
      return;
    }

    let nextStatus = targetCircle.status;
    let nextLatency = targetCircle.latency;
    const isPreviewEngine = advancedPreferences.value.experimentalTransport;
    const isInviteRelay = targetCircle.relay.startsWith("invite://");
    const activeSessionCount = sessions.value.filter((session) => {
      return session.circleId === circleId && !session.archived;
    }).length;

    if (action === "disconnect") {
      nextStatus = "closed";
      nextLatency = "--";
    } else if (action === "connect") {
      nextStatus = isPreviewEngine && !isInviteRelay ? "open" : "connecting";
      nextLatency = isPreviewEngine && !isInviteRelay ? buildLocalTransportLatency(targetCircle.relay, targetCircle.type) : "--";
    } else if (action === "discoverPeers") {
      nextStatus =
        isPreviewEngine && activeSessionCount > 0 && !isInviteRelay
          ? "open"
          : targetCircle.status === "closed"
            ? "connecting"
            : targetCircle.status;
      nextLatency =
        isPreviewEngine && activeSessionCount > 0 && !isInviteRelay
          ? buildLocalTransportLatency(targetCircle.relay, targetCircle.type)
          : targetCircle.status === "closed"
            ? "--"
            : targetCircle.latency;
    } else {
      nextStatus = "open";
      nextLatency = buildLocalTransportLatency(targetCircle.relay, targetCircle.type);
    }

    circles.value = circles.value.map((circle) => {
      if (circle.id !== circleId) {
        return circle;
      }

      return {
        ...circle,
        status: nextStatus,
        latency: nextLatency,
      };
    });

    if (action !== "syncSessions") {
      return;
    }

    let primarySessionId = "";
    sessions.value = sessions.value.map((session) => {
      if (session.circleId !== circleId || session.archived) {
        return session;
      }

      if (!primarySessionId) {
        primarySessionId = session.id;
      }

      return {
        ...session,
        unreadCount: undefined,
        time: "synced",
      };
    });

    if (!primarySessionId) {
      return;
    }

    const body = advancedPreferences.value.useTorNetwork
      ? advancedPreferences.value.experimentalTransport
        ? "Native preview transport synced this circle through the privacy path."
        : "Session sync completed through the privacy relay path."
      : advancedPreferences.value.experimentalTransport
        ? "Native preview transport synced this circle through the local runtime."
        : "Session sync completed across discovered relay peers.";
    const systemMessage: MessageItem = {
      id: `${primarySessionId}-sync-${Date.now()}`,
      kind: "system",
      author: "system",
      body,
      time: "now",
    };

    messageStore.value = {
      ...messageStore.value,
      [primarySessionId]: [...(messageStore.value[primarySessionId] ?? []), systemMessage],
    };
    sessions.value = sessions.value.map((session) => {
      if (session.id !== primarySessionId) {
        return session;
      }

      return {
        ...session,
        subtitle: body,
      };
    });
  }

  async function runTransportCircleAction(circleId: string, action: TransportCircleAction) {
    if (!circles.value.some((circle) => circle.id === circleId)) {
      return;
    }

    transportBusyCircleId.value = circleId;

    try {
      const result = await applyTransportCircleAction({
        circleId,
        action,
        activeCircleId: activeCircleId.value || undefined,
        useTorNetwork: advancedPreferences.value.useTorNetwork,
        experimentalTransport: advancedPreferences.value.experimentalTransport,
      });

      if (result.kind === "applied") {
        applyTransportMutationResult(result.result, {
          preferredCircleId: activeCircleId.value,
          preferredSessionId: selectedSessionId.value,
        });
        return;
      }

      if (result.kind === "blocked") {
        showTransportNotice({
          id: `runtime-blocked-${circleId}-${result.code}-${result.message}`,
          tone: "warn",
          title: `${circleLabelForRuntimeNotice(circleId)} runtime command unavailable`,
          detail: result.message,
          circleId,
        });
        await refreshTransportSnapshot({ suppressNotice: true });
        return;
      }

      applyLocalTransportCircleAction(circleId, action);
      await refreshTransportSnapshot({
        pendingActivity: { circleId, action },
      });
    } finally {
      if (transportBusyCircleId.value === circleId) {
        transportBusyCircleId.value = "";
      }
    }
  }

  function updateAppPreferences(patch: Partial<AppPreferences>) {
    appPreferences.value = {
      ...appPreferences.value,
      ...patch,
    };
  }

  function updateNotificationPreferences(patch: Partial<NotificationPreferences>) {
    notificationPreferences.value = {
      ...notificationPreferences.value,
      ...patch,
    };
  }

  function updateAdvancedPreferences(patch: Partial<AdvancedPreferences>) {
    advancedPreferences.value = {
      ...advancedPreferences.value,
      ...patch,
    };
  }

  function openCircleDirectoryFromSettings() {
    replaceTopOverlayPage({ kind: "circle-directory" });
  }

  function pushOverlayPage(page: OverlayPage) {
    applyOverlayPages([...overlayPages.value, page], { mode: "push" });
  }

  function replaceTopOverlayPage(page: OverlayPage) {
    applyOverlayPages([...overlayPages.value.slice(0, -1), page]);
  }

  function closeTopOverlayPage() {
    if (!overlayPages.value.length) {
      return;
    }

    if (overlayNavigationReady && typeof window !== "undefined") {
      window.history.back();
      return;
    }

    applyOverlayPages(overlayPages.value.slice(0, -1), { mode: "history" });
  }

  function closeAllOverlayPages() {
    if (!overlayPages.value.length) {
      return;
    }

    if (overlayNavigationReady && typeof window !== "undefined" && overlayHistoryDepth > 0) {
      window.history.go(-overlayHistoryDepth);
      return;
    }

    applyOverlayPages([], { historyDepth: 0 });
  }

  return {
    searchText,
    composerText,
    isAuthenticated,
    userProfile,
    showLaunch,
    showCircleSwitcher,
    showSettingsDrawer,
    showDetailsDrawer,
    circles,
    sessions,
    contacts,
    appPreferences,
    notificationPreferences,
    advancedPreferences,
    activeCircleId,
    selectedSessionId,
    bootstrapStatus,
    activeCircle,
    archivedSessionsForCircle,
    currentCircleContactIds,
    filteredSessions,
    selectedSession,
    activeMessages,
    canLoadOlderMessages,
    loadingOlderMessages,
    selectedContact,
    selectedGroup,
    selectedGroupMembers,
    transportSnapshot,
    transportNotice,
    activeTransportDiagnostic,
    activeOverlayPage,
    activeOverlayContact,
    activeOverlayCircle,
    activeOverlayTransportDiagnostic,
    activeOverlayDiscoveredPeers,
    activeOverlaySessionSyncItems,
    activeOverlayTransportActivities,
    activeOverlayRuntimeSessions,
    isActiveOverlayTransportBusy,
    activeOverlayCircleSessionCount,
    activeOverlayCircleDirectCount,
    activeOverlayCircleGroupCount,
    activeOverlayCircleArchivedCount,
    activeOverlayGroupSession,
    activeOverlayGroup,
    activeOverlayGroupMembers,
    activeOverlayGroupAvailableContacts,
    activeOverlayGroupCreateContacts,
    inviteLink,
    selectSession,
    chooseCircle,
    toggleCircleSwitcher,
    openNewMessage,
    openFindPeoplePage,
    openCircleManagement,
    openCircleDetail,
    openDetailsDrawer,
    openContactProfile,
    openGroupSelectMembersPage,
    openGroupCreatePage,
    openProfilePage,
    updateComposerText,
    loadOlderMessages,
    sendPreviewMessage,
    retryMessageDelivery,
    startConversation,
    startSelfChat,
    createGroupChat,
    startLookupChat,
    joinCircleFromLookup,
    handleSettingsAction,
    closeCircleOverlay,
    completeLogin,
    logout,
    handleSessionAction,
    openArchivedSession,
    toggleContactBlock,
    toggleGroupMute,
    leaveGroup,
    openMemberProfile,
    openGroupNamePage,
    openGroupMembersPage,
    openGroupAddMembersPage,
    openGroupRemoveMembersPage,
    updateGroupName,
    updateGroupMembers,
    sendMessageFromProfile,
    openArchivedPage,
    addCircleFromDirectory,
    updateCircle,
    removeCircle,
    runTransportCircleAction,
    updateAppPreferences,
    updateNotificationPreferences,
    updateAdvancedPreferences,
    openCircleDirectoryFromSettings,
    closeTopOverlayPage,
    dismissTransportNotice,
  };
}
