import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import {
  createEmptyShellState,
  createShellStateFromDomainSeed,
} from "../../data/shellDefaults";
import { loadChatDomainSeed, saveChatDomainSeed } from "../../services/chatDomain";
import {
  addChatCircle,
  applyChatSessionAction,
  removeChatCircle,
  sendChatMessage,
  startDirectConversation,
  toggleChatContactBlock,
  updateChatCircle,
} from "../../services/chatMutations";
import { loadShellState, saveShellState } from "../../services/shellState";
import {
  applyTransportCircleAction,
  loadTransportSnapshot,
} from "../../services/transportDiagnostics";
import type {
  AdvancedPreferences,
  AddCircleInput,
  AppPreferences,
  ChatDomainSeed,
  ContactItem,
  DiscoveredPeer,
  GroupProfile,
  MessageItem,
  NotificationPreferences,
  PersistedShellState,
  SessionSyncItem,
  SettingPageId,
  SessionAction,
  SessionItem,
  TransportActivityItem,
  TransportCircleAction,
  TransportMutationResult,
  TransportSnapshot,
  UpdateCircleInput,
} from "../../types/chat";

type BootstrapStatus = {
  project: string;
  phase: string;
  ready: boolean;
  stack: string[];
  next: string[];
};

type OverlayPage =
  | { kind: "circle-directory" }
  | { kind: "circle-detail"; circleId: string }
  | { kind: "settings-detail"; settingId: SettingPageId }
  | { kind: "new-message" }
  | { kind: "find-people" }
  | { kind: "archived" }
  | { kind: "contact"; contactId: string }
  | { kind: "group"; sessionId: string };

export function useChatShell() {
  const initialShellState = createEmptyShellState();
  const isShellStateReady = ref(false);
  let persistTimer: ReturnType<typeof window.setTimeout> | null = null;
  let transportTimer: ReturnType<typeof window.setTimeout> | null = null;
  let transportHeartbeatTimer: ReturnType<typeof window.setInterval> | null = null;
  const transportBusyCircleId = ref("");

  const searchText = ref("");
  const composerText = ref("");
  const isAuthenticated = ref(initialShellState.isAuthenticated);
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
  const overlayPages = ref<OverlayPage[]>([]);
  const transportSnapshot = ref<TransportSnapshot | null>(null);

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

  const activeOverlayGroupSession = computed(() => {
    const page = activeOverlayPage.value;
    if (page?.kind !== "group") {
      return null;
    }

    return sessions.value.find((session) => session.id === page.sessionId) ?? null;
  });

  const activeOverlayGroup = computed(() => {
    const page = activeOverlayPage.value;
    if (page?.kind !== "group") {
      return null;
    }

    return groups.value.find((group) => group.sessionId === page.sessionId) ?? null;
  });

  const activeOverlayGroupMembers = computed(() => {
    if (!activeOverlayGroup.value) {
      return [];
    }

    return activeOverlayGroup.value.members
      .map((member) => contacts.value.find((item) => item.id === member.contactId))
      .filter((item): item is ContactItem => !!item);
  });

  const inviteLink = computed(() => {
    if (!activeCircle.value) {
      return "p2pchat://circle";
    }

    return `p2pchat://circle/${activeCircle.value.id}?relay=${encodeURIComponent(activeCircle.value.relay)}`;
  });

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
    [
      isAuthenticated,
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

  onBeforeUnmount(() => {
    if (persistTimer) {
      window.clearTimeout(persistTimer);
    }

    if (transportTimer) {
      window.clearTimeout(transportTimer);
    }

    stopTransportHeartbeat();
  });

  onMounted(async () => {
    try {
      bootstrapStatus.value = await invoke<BootstrapStatus>("bootstrap_status");
    } catch {
      bootstrapStatus.value = null;
    }

    const domainSeed = await loadChatDomainSeed(createEmptyShellState());
    const fallbackState = createShellStateFromDomainSeed(domainSeed);
    const persistedState = await loadShellState(fallbackState);
    applyShellState(persistedState);
    isShellStateReady.value = true;
    await refreshTransportSnapshot();
    schedulePersistence();

    window.setTimeout(() => {
      showLaunch.value = false;
    }, 950);
  });

  function snapshotShellState(): PersistedShellState {
    return JSON.parse(
      JSON.stringify({
        isAuthenticated: isAuthenticated.value,
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
      }),
    ) as PersistedShellState;
  }

  function toChatDomainSeed(state: PersistedShellState): ChatDomainSeed {
    return {
      circles: state.circles,
      contacts: state.contacts,
      sessions: state.sessions,
      groups: state.groups,
      messageStore: state.messageStore,
    };
  }

  async function persistState() {
    const shellState = snapshotShellState();
    await Promise.all([saveShellState(shellState), saveChatDomainSeed(toChatDomainSeed(shellState))]);
  }

  async function refreshTransportSnapshot(pendingActivity?: {
    circleId: string;
    action: TransportCircleAction;
  }) {
    transportSnapshot.value = await loadTransportSnapshot(
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
        pendingActivity,
      },
    );
  }

  function stopTransportHeartbeat() {
    if (transportHeartbeatTimer) {
      window.clearInterval(transportHeartbeatTimer);
      transportHeartbeatTimer = null;
    }
  }

  function restartTransportHeartbeat() {
    stopTransportHeartbeat();

    if (!isAuthenticated.value) {
      return;
    }

    if (
      !advancedPreferences.value.relayDiagnostics &&
      !advancedPreferences.value.experimentalTransport
    ) {
      return;
    }

    const intervalMs = advancedPreferences.value.relayDiagnostics ? 12000 : 16000;
    transportHeartbeatTimer = window.setInterval(() => {
      void refreshTransportSnapshot();
    }, intervalMs);
  }

  function applyTransportMutationResult(
    result: TransportMutationResult,
    options?: { preferredCircleId?: string; preferredSessionId?: string },
  ) {
    applyDomainSeed(result.seed, options);
    transportSnapshot.value = result.snapshot;
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

  function applyDomainSeed(
    seed: ChatDomainSeed,
    options?: { preferredCircleId?: string; preferredSessionId?: string },
  ) {
    const nextCircles = seed.circles.length ? seed.circles : initialShellState.circles;
    const nextCircleId = nextCircles.some((circle) => circle.id === options?.preferredCircleId)
      ? options?.preferredCircleId ?? ""
      : nextCircles.some((circle) => circle.id === activeCircleId.value)
        ? activeCircleId.value
        : nextCircles[0]?.id ?? "";

    circles.value = nextCircles;
    activeCircleId.value = nextCircleId;
    sessions.value = seed.sessions;
    contacts.value = seed.contacts;
    groups.value = seed.groups;
    messageStore.value = seed.messageStore;

    const visibleSessions = seed.sessions.filter((session) => {
      return session.circleId === nextCircleId && !session.archived;
    });
    const nextSessionId = visibleSessions.some((session) => session.id === options?.preferredSessionId)
      ? options?.preferredSessionId ?? ""
      : visibleSessions.some((session) => session.id === selectedSessionId.value)
        ? selectedSessionId.value
        : visibleSessions[0]?.id ?? "";

    selectedSessionId.value = nextSessionId;
  }

  function applyShellState(state: PersistedShellState) {
    isAuthenticated.value = state.isAuthenticated;
    appPreferences.value = state.appPreferences;
    notificationPreferences.value = state.notificationPreferences;
    advancedPreferences.value = state.advancedPreferences;
    applyDomainSeed(toChatDomainSeed(state), {
      preferredCircleId: state.activeCircleId,
      preferredSessionId: state.selectedSessionId,
    });
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
    showCircleSwitcher.value = false;
    showDetailsDrawer.value = false;
    closeAllOverlayPages();
  }

  function toggleCircleSwitcher() {
    showCircleSwitcher.value = !showCircleSwitcher.value;
  }

  function openNewMessage() {
    pushOverlayPage({ kind: "new-message" });
  }

  function openFindPeoplePage() {
    pushOverlayPage({ kind: "find-people" });
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
    pushOverlayPage({ kind: "contact", contactId });
  }

  function openGroupProfilePage(sessionId: string) {
    pushOverlayPage({ kind: "group", sessionId });
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

  function completeLogin() {
    isAuthenticated.value = true;
  }

  function logout() {
    isAuthenticated.value = false;
    showSettingsDrawer.value = false;
    showCircleSwitcher.value = false;
    showDetailsDrawer.value = false;
    closeAllOverlayPages();
    composerText.value = "";
    searchText.value = "";
  }

  function deleteSession(sessionId: string) {
    sessions.value = sessions.value.filter((session) => session.id !== sessionId);
    delete messageStore.value[sessionId];
  }

  function applyLocalSendPreviewMessage(content: string, sessionId: string) {
    const message: MessageItem = {
      id: `local-${Date.now()}`,
      kind: "text",
      author: "me",
      body: content,
      time: "now",
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
    const nextSeed = await sendChatMessage({ sessionId, body: content });
    composerText.value = "";

    if (nextSeed) {
      applyDomainSeed(nextSeed, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: sessionId,
      });
      return;
    }

    applyLocalSendPreviewMessage(content, sessionId);
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

    const sessionId = `session-${contact.id}`;
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

    if (removedSessionIds.has(selectedSessionId.value)) {
      selectedSessionId.value = "";
    }

    if (removedActiveCircle) {
      activeCircleId.value = remainingCircles[0]?.id ?? "";
      showDetailsDrawer.value = false;
      closeAllOverlayPages();
      return;
    }

    overlayPages.value = overlayPages.value.filter((page) => {
      if (page.kind === "circle-detail") {
        return page.circleId !== circleId;
      }

      if (page.kind === "group") {
        return !removedSessionIds.has(page.sessionId);
      }

      return true;
    });
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
        showDetailsDrawer.value = false;
        closeAllOverlayPages();
        return;
      }

      overlayPages.value = overlayPages.value.filter((page) => {
        if (page.kind === "circle-detail") {
          return page.circleId !== circleId;
        }

        if (page.kind === "group") {
          return !removedSessionIds.has(page.sessionId);
        }

        return true;
      });
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

      if (result) {
        applyTransportMutationResult(result, {
          preferredCircleId: activeCircleId.value,
          preferredSessionId: selectedSessionId.value,
        });
        return;
      }

      applyLocalTransportCircleAction(circleId, action);
      await refreshTransportSnapshot({ circleId, action });
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
    overlayPages.value = [...overlayPages.value, page];
    showCircleSwitcher.value = false;
    showSettingsDrawer.value = false;
    showDetailsDrawer.value = false;
  }

  function replaceTopOverlayPage(page: OverlayPage) {
    overlayPages.value = [...overlayPages.value.slice(0, -1), page];
    showCircleSwitcher.value = false;
    showSettingsDrawer.value = false;
    showDetailsDrawer.value = false;
  }

  function closeTopOverlayPage() {
    overlayPages.value = overlayPages.value.slice(0, -1);
  }

  function closeAllOverlayPages() {
    overlayPages.value = [];
  }

  return {
    searchText,
    composerText,
    isAuthenticated,
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
    selectedContact,
    selectedGroup,
    selectedGroupMembers,
    transportSnapshot,
    activeTransportDiagnostic,
    activeOverlayPage,
    activeOverlayContact,
    activeOverlayCircle,
    activeOverlayTransportDiagnostic,
    activeOverlayDiscoveredPeers,
    activeOverlaySessionSyncItems,
    activeOverlayTransportActivities,
    isActiveOverlayTransportBusy,
    activeOverlayCircleSessionCount,
    activeOverlayCircleDirectCount,
    activeOverlayCircleGroupCount,
    activeOverlayCircleArchivedCount,
    activeOverlayGroupSession,
    activeOverlayGroup,
    activeOverlayGroupMembers,
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
    openProfilePage,
    sendPreviewMessage,
    startConversation,
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
  };
}
