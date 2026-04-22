import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  createChatShellSnapshotFromPersistedState,
  createEmptyShellState,
  createLoggedOutShellState,
} from "../../data/shellDefaults";
import { createChatSeedFallback } from "../../mock/chatSeedFallback";
import {
  addChatCircle,
  applyChatSessionAction,
  cacheChatMessageMedia,
  cleanupChatMediaAssets,
  createGroupConversation,
  removeChatCircle,
  restoreChatCircle,
  retryChatMessageDelivery,
  sendChatFileMessage,
  sendChatImageMessage,
  sendChatVideoMessage,
  sendChatMessage,
  startDirectConversation,
  startLookupConversation,
  startSelfConversation,
  storeChatMediaAsset,
  toggleChatContactBlock,
  updateChatContactRemark,
  updateChatGroupMembers,
  updateChatGroupName,
  updateChatSessionDraft,
  updateChatCircle,
} from "../../services/chatMutations";
import {
  bootstrapAuthSession,
  completeLogin as completeLoginViaRuntime,
  loadChatDomainOverview,
  loadChatDomainOverviewLocally,
  loadChatSessionMessageUpdates,
  loadChatSessionMessageUpdatesLocally,
  loadChatSessionMessages,
  loadChatSessionMessagesLocally,
  loadChatShellSnapshot,
  loadChatShellSnapshotLocally,
  logoutChatSession,
  persistChatShellSnapshotLocally,
  saveChatShellSnapshot,
  syncAuthRuntime as syncAuthRuntimeViaRuntime,
  updateAuthRuntime as updateAuthRuntimeViaRuntime,
} from "../../services/chatShell";
import {
  buildAuthRuntimeBindingSummary,
  buildUpdatedAuthRuntime,
  deriveAuthRuntimeFromAuthSession,
} from "../../services/authRuntime";
import { classifyChatQuery, isCircleQuery } from "../../services/chatQueryIntents";
import {
  encodeFileMessageMeta,
  fileMessageLocalPath,
  fileMessageMetaLabel,
  fileMessageRemoteUrl,
} from "../chat/fileMessageMeta";
import {
  applyTransportCircleAction,
  deriveRuntimeRecoveryAction,
  loadTransportSnapshot,
  loadTransportSnapshotLocally,
} from "../../services/transportDiagnostics";
import {
  encodeImageMessageMeta,
  imageMessageLocalPath,
  imageMessageMetaLabel,
  imageMessageRemoteUrl,
} from "../chat/imageMessageMeta";
import {
  encodeVideoMessageMeta,
  videoMessageLocalPath,
  videoMessageMetaLabel,
  videoMessageRemoteUrl,
} from "../chat/videoMessageMeta";
import {
  normalizeNostrPubkey,
  resolveMessageAuthorLabel,
  resolveReplyPreviewAuthorLabel,
} from "../chat/messageAuthor";
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
  AuthRuntimeSummary,
  AuthRuntimeBindingSummary,
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
  RestorableCircleEntry,
  ShellStateSnapshot,
  SessionSyncItem,
  SettingPageId,
  SessionAction,
  SessionItem,
  StoreChatMediaAssetInput,
  StoredChatMediaAsset,
  TransportActivityItem,
  TransportCircleAction,
  TransportMutationResult,
  TransportRuntimeSession,
  TransportSnapshot,
  UpdateCircleInput,
  UpdateContactRemarkInput,
  UpdateAuthRuntimeInput,
  UpdateGroupMembersInput,
  UpdateGroupNameInput,
  SendImageMessageInput,
  SendVideoMessageInput,
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
const AUTH_RUNTIME_SYNC_INTERVAL_MS = 2500;
const TRANSPORT_RELAY_HEARTBEAT_INTERVAL_MS = 2_500;
const TRANSPORT_RELAY_BACKGROUND_HEARTBEAT_INTERVAL_MS = 8_000;
const TRANSPORT_RELAY_DIAGNOSTIC_HEARTBEAT_INTERVAL_MS = 6_000;
const TRANSPORT_RELAY_DIAGNOSTIC_BACKGROUND_HEARTBEAT_INTERVAL_MS = 12_000;
const TRANSPORT_RUNTIME_RECOVERY_HEARTBEAT_INTERVAL_MS = 2_000;
const TRANSPORT_PUBLISH_WARN_TITLES = new Set([
  "Relay rejected event",
  "Relay closed publish connection",
  "Relay publish timed out",
  "Relay publish failed",
]);

function cloneState<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function hasTauriRuntime() {
  const globalWindow = globalThis as typeof globalThis & {
    __TAURI__?: unknown;
    __TAURI_INTERNALS__?: unknown;
  };

  return typeof window !== "undefined" && ("__TAURI_INTERNALS__" in globalWindow || "__TAURI__" in globalWindow);
}

function documentHidden() {
  return typeof document !== "undefined" && document.hidden;
}

export function useChatShell() {
  const initialShellState = createEmptyShellState();
  const isShellStateReady = ref(false);
  let persistTimer: number | null = null;
  let draftPersistTimer: number | null = null;
  let transportTimer: number | null = null;
  let transportHeartbeatTimer: number | null = null;
  let transportNoticeTimer: number | null = null;
  let authRuntimeSyncTimer: number | null = null;
  let authRuntimeNativeSyncSupported = true;
  let authRuntimeBackgroundSyncFailed = false;
  let transportSnapshotBackgroundRefreshFailed = false;
  let domainOverviewBackgroundRefreshFailed = false;
  let shellSnapshotPersistenceFailed = false;
  let draftRuntimePersistenceFailed = false;
  let sessionMessageUpdatesFailedSessionId = "";
  let activeTransportHeartbeatIntervalMs = 0;
  let latestDraftMutationSerial = 0;
  let pendingDraftSessionId: string | null = null;
  let pendingDraftValue = "";
  const transportBusyCircleId = ref("");

  const searchText = ref("");
  const composerText = ref("");
  const replyToMessageId = ref<string | null>(null);
  const findPeopleSubmitting = ref(false);
  const findPeopleErrorMessage = ref("");
  const isAuthenticated = ref(initialShellState.isAuthenticated);
  const authSession = ref(initialShellState.authSession);
  const authRuntime = ref(initialShellState.authRuntime);
  const authRuntimeBinding = ref<AuthRuntimeBindingSummary | null>(initialShellState.authRuntimeBinding);
  const userProfile = ref<UserProfile>(initialShellState.userProfile);
  const restorableCircles = ref(initialShellState.restorableCircles);
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

  const effectiveAuthRuntime = computed(() => {
    return authRuntime.value ?? deriveAuthRuntimeFromAuthSession(authSession.value);
  });

  const sendBlockedReason = computed(() => {
    return selectedSessionSendState.value.reason;
  });

  const canSendMessages = computed(() => {
    return selectedSessionSendState.value.canSend;
  });

  const runtimeDiagnosticError = computed(() => {
    return effectiveAuthRuntime.value?.error?.trim() ?? "";
  });

  const activeMessages = computed(() => {
    if (!selectedSession.value) {
      return [];
    }

    return messageStore.value[selectedSession.value.id] ?? [];
  });

  const replyingToMessage = computed(() => {
    const sessionId = selectedSession.value?.id;
    if (!sessionId || !replyToMessageId.value) {
      return null;
    }

    return (
      messageStore.value[sessionId]?.find((message) => message.id === replyToMessageId.value) ??
      null
    );
  });

  const mentionableContacts = computed<ContactItem[]>(() => {
    if (!selectedSession.value) {
      return [];
    }

    if (selectedSession.value.kind === "direct") {
      return selectedContact.value ? [selectedContact.value] : [];
    }

    if (selectedSession.value.kind === "group") {
      return selectedGroupMembers.value;
    }

    return [];
  });

  const activeComposerMentionMatch = computed(() => {
    if (!selectedSession.value) {
      return null;
    }

    const match = composerText.value.match(/(^|\s)@([A-Za-z0-9_.-]*)$/);
    if (!match) {
      return null;
    }

    return {
      prefix: match[1] ?? "",
      query: (match[2] ?? "").toLowerCase(),
      mentionStart: composerText.value.length - match[0].length + (match[1]?.length ?? 0),
    };
  });

  const mentionSuggestions = computed<ContactItem[]>(() => {
    const match = activeComposerMentionMatch.value;
    if (!match) {
      return [];
    }

    const seenContactIds = new Set<string>();
    return mentionableContacts.value
      .filter((contact) => {
        if (seenContactIds.has(contact.id)) {
          return false;
        }

        seenContactIds.add(contact.id);
        if (!match.query) {
          return true;
        }

        const normalizedHandle = contact.handle.toLowerCase();
        const normalizedName = contact.name.toLowerCase();
        const normalizedSubtitle = contact.subtitle.toLowerCase();
        return (
          normalizedHandle.includes(`@${match.query}`) ||
          normalizedName.includes(match.query) ||
          normalizedSubtitle.includes(match.query)
        );
      })
      .slice(0, 6);
  });

  const showMentionSuggestions = computed(() => {
    return !!activeComposerMentionMatch.value && mentionSuggestions.value.length > 0;
  });

  const mentionSelectionIndex = ref(0);

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

  const selectedSessionSendState = computed(() => {
    if (!isAuthenticated.value) {
      return {
        canSend: false,
        reason: "Log in before sending messages.",
      };
    }

    const runtime = effectiveAuthRuntime.value;
    if (runtime && !runtime.canSendMessages) {
      return {
        canSend: false,
        reason: runtime.sendBlockedReason ?? "",
      };
    }

    if (selectedSession.value?.kind === "direct" && selectedContact.value?.blocked) {
      return {
        canSend: false,
        reason: "This user is blocked. Unblock them to send messages.",
      };
    }

    if (selectedSession.value?.kind === "group") {
      if (selectedGroup.value?.needsJoin) {
        return {
          canSend: false,
          reason: "Join this group before sending messages.",
        };
      }

      if (selectedGroup.value?.canSend === false) {
        return {
          canSend: false,
          reason: "Sending is unavailable in this group right now.",
        };
      }
    }

    return {
      canSend: true,
      reason: "",
    };
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

  const activeOverlayMessageSession = computed(() => {
    const page = activeOverlayPage.value;
    if (page?.kind !== "message-detail") {
      return null;
    }

    return sessions.value.find((session) => session.id === page.sessionId) ?? null;
  });

  const activeOverlayMessage = computed(() => {
    const page = activeOverlayPage.value;
    if (page?.kind !== "message-detail") {
      return null;
    }

    return (
      messageStore.value[page.sessionId]?.find((message) => message.id === page.messageId) ?? null
    );
  });

  const activeOverlayMessageReplyTarget = computed(() => {
    const session = activeOverlayMessageSession.value;
    const message = activeOverlayMessage.value;
    if (!session || !message?.replyTo) {
      return null;
    }

    const sessionMessages = messageStore.value[session.id] ?? [];
    return (
      sessionMessages.find((candidate) => {
        return (
          candidate.id === message.replyTo?.messageId ||
          candidate.remoteId === message.replyTo?.remoteId ||
          candidate.signedNostrEvent?.eventId === message.replyTo?.remoteId
        );
      }) ?? null
    );
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

  function describeCommandError(error: unknown, fallback: string) {
    if (typeof error === "string" && error.trim()) {
      return error.trim();
    }

    if (error instanceof Error && error.message.trim()) {
      return error.message.trim();
    }

    if (error && typeof error === "object") {
      const message = Reflect.get(error, "message");
      if (typeof message === "string" && message.trim()) {
        return message.trim();
      }
    }

    return fallback;
  }

  function sessionActionFailureTitle(action: SessionAction) {
    switch (action) {
      case "pin":
        return "Pin update failed";
      case "mute":
        return "Mute update failed";
      case "archive":
        return "Archive action failed";
      case "unarchive":
        return "Restore chat failed";
      case "delete":
        return "Delete chat failed";
      default:
        return "Chat action failed";
    }
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

  function buildTransportActivityNotice(
    previousSnapshot: TransportSnapshot,
    snapshot: TransportSnapshot,
  ): TransportNotice | null {
    const previousActivityIds = new Set(previousSnapshot.activities.map((activity) => activity.id));
    const nextActivity = [...snapshot.activities]
      .reverse()
      .find((activity) => {
        return (
          !previousActivityIds.has(activity.id) &&
          activity.kind === "runtime" &&
          activity.level === "warn" &&
          TRANSPORT_PUBLISH_WARN_TITLES.has(activity.title)
        );
      });
    if (!nextActivity) {
      return null;
    }

    return {
      id: `transport-activity-${nextActivity.id}`,
      tone: "warn",
      title: `${circleLabelForRuntimeNotice(nextActivity.circleId)}: ${nextActivity.title}`,
      detail: nextActivity.detail,
      circleId: nextActivity.circleId,
    };
  }

  function maybeShowTransportNoticeFromSnapshot(
    previousSnapshot: TransportSnapshot | null,
    snapshot: TransportSnapshot,
  ) {
    if (!previousSnapshot) {
      return;
    }

    const activityNotice = buildTransportActivityNotice(previousSnapshot, snapshot);
    if (activityNotice) {
      showTransportNotice(activityNotice);
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
        return true;
      case "message-detail":
        return (
          advancedPreferences.value.showMessageInfo &&
          sessions.value.some((session) => session.id === page.sessionId) &&
          !!messageStore.value[page.sessionId]?.some((message) => message.id === page.messageId)
        );
      case "new-message":
      case "circle-invite":
      case "self-chat-confirm":
      case "group-select-members":
      case "archived":
        return !!activeCircle.value;
      case "find-people":
        return page.mode === "join-circle" || !!activeCircle.value;
      case "group-create":
        return (
          !!activeCircle.value &&
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

      replyToMessageId.value = null;
      composerText.value = selectedSession.value?.draft ?? "";
      void ensureSessionMessagesLoaded(sessionId);
    },
    { immediate: true },
  );

  watch(
    [showMentionSuggestions, mentionSuggestions],
    () => {
      if (!showMentionSuggestions.value) {
        mentionSelectionIndex.value = 0;
        return;
      }

      mentionSelectionIndex.value = Math.min(
        mentionSelectionIndex.value,
        Math.max(mentionSuggestions.value.length - 1, 0),
      );
    },
    { deep: true },
  );

  watch(
    [isAuthenticated, circles, contacts, sessions, groups, advancedPreferences, messageStore],
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
      authRuntime,
      userProfile,
      restorableCircles,
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

  watch(
    [isAuthenticated, authSession, authRuntime, authRuntimeBinding],
    () => {
      scheduleAuthRuntimeSync();
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
    clearAuthRuntimeSyncTimer();

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
    if (!hasTauriRuntime()) {
      bootstrapStatus.value = null;
    } else {
      try {
        bootstrapStatus.value = await invoke<BootstrapStatus>("bootstrap_status");
      } catch (error) {
        bootstrapStatus.value = null;
        showTransportNotice({
          id: `bootstrap-status-failed-${Date.now()}`,
          tone: "warn",
          title: "Bootstrap status unavailable",
          detail: describeCommandError(
            error,
            "Desktop bootstrap metadata could not be loaded. Settings pages may show reduced startup diagnostics.",
          ),
        });
      }
    }

    const fallbackState = createChatSeedFallback();
    let snapshot: ChatShellSnapshot;

    try {
      snapshot = await loadChatShellSnapshot(fallbackState);
    } catch (error) {
      showTransportNotice({
        id: `load-shell-snapshot-failed-${Date.now()}`,
        tone: "warn",
        title: "Desktop shell restore failed",
        detail: describeCommandError(
          error,
          "Desktop startup could not load the native shell snapshot, so the UI fell back to the local cached shell state.",
        ),
      });
      snapshot = loadChatShellSnapshotLocally(fallbackState);
    }

    applyChatShellSnapshot(snapshot);
    initializeOverlayNavigation();
    pagehideHandler = () => {
      flushPendingPersistence("full");
    };
    visibilitychangeHandler = () => {
      if (document.hidden) {
        flushPendingPersistence("full");
        restartTransportHeartbeat();
        return;
      }

      restartTransportHeartbeat();
      void refreshTransportSnapshot({ suppressNotice: true });
    };
    window.addEventListener("pagehide", pagehideHandler);
    document.addEventListener("visibilitychange", visibilitychangeHandler);
    isShellStateReady.value = true;
    await refreshTransportSnapshot({ showFailureNotice: true });
    schedulePersistence();

    window.setTimeout(() => {
      showLaunch.value = false;
    }, 950);
  });

  function snapshotShellState(): PersistedShellState {
    return cloneState({
      isAuthenticated: isAuthenticated.value,
      authSession: authSession.value,
      authRuntime: authRuntime.value,
      authRuntimeBinding: authRuntimeBinding.value,
      userProfile: userProfile.value,
      restorableCircles: restorableCircles.value,
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

  function clearAuthRuntimeSyncTimer() {
    if (authRuntimeSyncTimer) {
      window.clearTimeout(authRuntimeSyncTimer);
      authRuntimeSyncTimer = null;
    }
  }

  function shouldSyncAuthRuntime() {
    if (!authRuntimeNativeSyncSupported || !isAuthenticated.value) {
      return false;
    }

    return effectiveAuthRuntime.value?.state === "pending";
  }

  function authRuntimeSyncStateKey(
    state: Pick<ShellStateSnapshot, "isAuthenticated" | "authSession" | "authRuntime" | "authRuntimeBinding">,
  ) {
    return JSON.stringify({
      isAuthenticated: state.isAuthenticated,
      authSession: state.authSession,
      authRuntime: state.authRuntime,
      authRuntimeBinding: state.authRuntimeBinding,
    });
  }

  function applySyncedAuthRuntimeState(
    state: Pick<ShellStateSnapshot, "isAuthenticated" | "authSession" | "authRuntime" | "authRuntimeBinding">,
  ) {
    isAuthenticated.value = state.isAuthenticated;
    authSession.value = state.authSession;
    authRuntime.value = state.authRuntime;
    authRuntimeBinding.value = state.authRuntimeBinding;
  }

  async function syncAuthRuntimeState() {
    clearAuthRuntimeSyncTimer();

    if (!shouldSyncAuthRuntime()) {
      return;
    }

    let runtimeShell: ShellStateSnapshot | null = null;

    try {
      runtimeShell = await syncAuthRuntimeViaRuntime();
      authRuntimeBackgroundSyncFailed = false;
    } catch (error) {
      if (!authRuntimeBackgroundSyncFailed) {
        showTransportNotice({
          id: `background-auth-runtime-sync-failed-${Date.now()}`,
          tone: "warn",
          title: "Auth runtime background sync failed",
          detail: describeCommandError(
            error,
            "Desktop auth runtime sync is still failing in the background. The UI will keep retrying automatically.",
          ),
        });
      }

      authRuntimeBackgroundSyncFailed = true;
      scheduleAuthRuntimeSync();
      return;
    }

    if (!runtimeShell) {
      authRuntimeNativeSyncSupported = false;
      authRuntimeBackgroundSyncFailed = false;
      return;
    }

    if (authRuntimeSyncStateKey(runtimeShell) !== authRuntimeSyncStateKey({
      isAuthenticated: isAuthenticated.value,
      authSession: authSession.value,
      authRuntime: authRuntime.value,
      authRuntimeBinding: authRuntimeBinding.value,
    })) {
      applySyncedAuthRuntimeState(runtimeShell);
    }

    scheduleAuthRuntimeSync();
  }

  async function refreshAuthRuntimeStateFromNative(options: { showFailureNotice?: boolean } = {}) {
    if (!authRuntimeNativeSyncSupported || !isAuthenticated.value) {
      return;
    }

    let runtimeShell: ShellStateSnapshot | null = null;

    try {
      runtimeShell = await syncAuthRuntimeViaRuntime();
      authRuntimeBackgroundSyncFailed = false;
    } catch (error) {
      if (options.showFailureNotice) {
        showTransportNotice({
          id: `auth-runtime-sync-failed-${Date.now()}`,
          tone: "warn",
          title: "Auth runtime sync failed",
          detail: describeCommandError(
            error,
            "Desktop auth runtime sync did not complete. Try again after the signer or native runtime becomes reachable.",
          ),
        });
      }

      scheduleAuthRuntimeSync();
      return;
    }

    if (!runtimeShell) {
      authRuntimeNativeSyncSupported = false;
      authRuntimeBackgroundSyncFailed = false;
      return;
    }

    if (authRuntimeSyncStateKey(runtimeShell) !== authRuntimeSyncStateKey({
      isAuthenticated: isAuthenticated.value,
      authSession: authSession.value,
      authRuntime: authRuntime.value,
      authRuntimeBinding: authRuntimeBinding.value,
    })) {
      applySyncedAuthRuntimeState(runtimeShell);
    }

    scheduleAuthRuntimeSync();
  }

  async function syncAuthRuntimeNow() {
    await refreshAuthRuntimeStateFromNative({ showFailureNotice: true });
  }

  async function runFallbackEligibleMutation<T>(
    mutation: () => Promise<T | null>,
    desktopErrorNotice?: {
      title: string;
      fallbackDetail: string;
    },
  ): Promise<{ result: T | null; canFallbackLocally: boolean }> {
    try {
      return {
        result: await mutation(),
        canFallbackLocally: true,
      };
    } catch (error) {
      if (desktopErrorNotice) {
        showTransportNotice({
          id: `desktop-mutation-${Date.now()}-${desktopErrorNotice.title}`,
          tone: "warn",
          title: desktopErrorNotice.title,
          detail: describeCommandError(error, desktopErrorNotice.fallbackDetail),
        });
      }

      return {
        result: null,
        canFallbackLocally: false,
      };
    }
  }

  function scheduleAuthRuntimeSync() {
    clearAuthRuntimeSyncTimer();

    if (!shouldSyncAuthRuntime()) {
      authRuntimeBackgroundSyncFailed = false;
      return;
    }

    authRuntimeSyncTimer = window.setTimeout(() => {
      authRuntimeSyncTimer = null;
      void syncAuthRuntimeState();
    }, AUTH_RUNTIME_SYNC_INTERVAL_MS);
  }

  function resetDesktopFailureFlags() {
    authRuntimeNativeSyncSupported = true;
    authRuntimeBackgroundSyncFailed = false;
    transportSnapshotBackgroundRefreshFailed = false;
    domainOverviewBackgroundRefreshFailed = false;
    shellSnapshotPersistenceFailed = false;
    draftRuntimePersistenceFailed = false;
    sessionMessageUpdatesFailedSessionId = "";
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
    let nextSeed: ChatDomainSeed | null = null;

    try {
      nextSeed = await updateChatSessionDraft({ sessionId, draft });
      draftRuntimePersistenceFailed = false;
    } catch (error) {
      if (!draftRuntimePersistenceFailed) {
        showTransportNotice({
          id: `save-draft-failed-${sessionId}-${Date.now()}`,
          tone: "warn",
          title: "Draft save failed",
          detail: describeCommandError(
            error,
            "Desktop draft state could not be written to native storage, so only the local cached draft was updated.",
          ),
        });
      }

      draftRuntimePersistenceFailed = true;
      return;
    }

    if (!nextSeed || mutationSerial !== latestDraftMutationSerial) {
      return;
    }

    applyDomainSeed(nextSeed, {
      preferredCircleId: activeCircleId.value,
      preferredSessionId: selectedSessionId.value || sessionId,
    });
  }

  async function persistShellSnapshotToNative(
    snapshot: ChatShellSnapshot,
    options: { showFailureNotice?: boolean } = {},
  ) {
    try {
      await saveChatShellSnapshot(snapshot);
      shellSnapshotPersistenceFailed = false;
    } catch (error) {
      if (options.showFailureNotice !== false && !shellSnapshotPersistenceFailed) {
        showTransportNotice({
          id: `save-shell-snapshot-failed-${Date.now()}`,
          tone: "warn",
          title: "Desktop shell save failed",
          detail: describeCommandError(
            error,
            "Desktop shell state could not be written to native storage, so only the local cached snapshot was updated.",
          ),
        });
      }

      shellSnapshotPersistenceFailed = true;
    }
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
      void persistShellSnapshotToNative(snapshot);
    }
  }

  async function persistState() {
    await flushPendingDraftPersistence();
    await persistShellSnapshotToNative(snapshotChatShellState());
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
      showFailureNotice?: boolean;
      suppressNotice?: boolean;
    } = {},
  ) {
    const input = {
      activeCircleId: activeCircleId.value || undefined,
      useTorNetwork: advancedPreferences.value.useTorNetwork,
      experimentalTransport: advancedPreferences.value.experimentalTransport,
    };
    const fallback = {
      circles: circles.value,
      contacts: contacts.value,
      sessions: sessions.value,
      groups: groups.value,
      messageStore: messageStore.value,
      activeCircleId: activeCircleId.value,
      advanced: advancedPreferences.value,
      previousSnapshot: transportSnapshot.value,
      pendingActivity: options.pendingActivity,
    };
    let result: Awaited<ReturnType<typeof loadTransportSnapshot>>;
    let desktopLoadFailed = false;

    try {
      result = await loadTransportSnapshot(input, fallback);
      transportSnapshotBackgroundRefreshFailed = false;
    } catch (error) {
      desktopLoadFailed = true;
      if (options.showFailureNotice || !transportSnapshotBackgroundRefreshFailed) {
        showTransportNotice({
          id: `load-transport-snapshot-failed-${Date.now()}`,
          tone: "warn",
          title: "Transport status refresh failed",
          detail: describeCommandError(
            error,
            "Desktop transport diagnostics could not be refreshed, so the UI fell back to the locally cached transport snapshot.",
          ),
        });
      }

      transportSnapshotBackgroundRefreshFailed = true;
      result = {
        snapshot: loadTransportSnapshotLocally(fallback),
        source: "fallback" as const,
      };
    }

    setTransportSnapshot(result.snapshot, {
      suppressNotice: options.suppressNotice || desktopLoadFailed,
    });

    if (result.source === "tauri") {
      await refreshDomainOverview();
      await refreshSessionMessageUpdates(selectedSession.value?.id);
    }

    if (desktopLoadFailed) {
      return;
    }

    const recoveryAction = deriveRuntimeRecoveryAction(result.snapshot, circles.value);
    if (!recoveryAction || transportBusyCircleId.value) {
      return;
    }

    if (result.source === "tauri") {
      await runTransportCircleAction(recoveryAction.circleId, recoveryAction.action);
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

  function relaySupportsTransportHeartbeat(relay: string) {
    return /^(ws|wss):\/\//i.test(relay.trim());
  }

  function hasRelayTransportHeartbeatTarget() {
    if (transportSnapshot.value?.runtimeSessions.some((session) => relaySupportsTransportHeartbeat(session.endpoint))) {
      return true;
    }

    return circles.value.some((circle) => relaySupportsTransportHeartbeat(circle.relay));
  }

  function desiredTransportHeartbeatIntervalMs() {
    if (!isAuthenticated.value) {
      return 0;
    }

    if (!hasRelayTransportHeartbeatTarget()) {
      return 0;
    }

    const runtimeRecoveryActive = !!transportSnapshot.value?.runtimeSessions.some((session) => {
      return session.queueState === "queued" || session.queueState === "backoff";
    });
    if (runtimeRecoveryActive) {
      return TRANSPORT_RUNTIME_RECOVERY_HEARTBEAT_INTERVAL_MS;
    }

    const background = documentHidden();

    return advancedPreferences.value.relayDiagnostics
      ? background
        ? TRANSPORT_RELAY_DIAGNOSTIC_BACKGROUND_HEARTBEAT_INTERVAL_MS
        : TRANSPORT_RELAY_DIAGNOSTIC_HEARTBEAT_INTERVAL_MS
      : background
        ? TRANSPORT_RELAY_BACKGROUND_HEARTBEAT_INTERVAL_MS
        : TRANSPORT_RELAY_HEARTBEAT_INTERVAL_MS;
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

  function sessionSubtitleFromMessage(sessionId: string, message: MessageItem) {
    const session = sessions.value.find((item) => item.id === sessionId) ?? null;
    if (session?.kind === "group" && message.author === "peer") {
      const authorLabel = resolveMessageAuthorLabel(session, message);
      return `${authorLabel}: ${message.body}`;
    }

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
    const hydratedMessages = hydrateSessionMessages(sessionId, messages);
    messageStore.value = {
      ...messageStore.value,
      [sessionId]: hydratedMessages,
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

    const input = {
      sessionId,
      limit: SESSION_MESSAGE_PAGE_SIZE,
    };
    let page;

    try {
      page = await loadChatSessionMessages(input, messageStore.value);
    } catch (error) {
      showTransportNotice({
        id: `load-session-messages-failed-${sessionId}-${Date.now()}`,
        tone: "warn",
        title: "Message history load failed",
        detail: describeCommandError(
          error,
          "Desktop message history could not be loaded, so the UI fell back to the locally cached messages.",
        ),
      });
      page = loadChatSessionMessagesLocally(input, messageStore.value);
    }

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
    let hadDesktopRefreshFailure = false;
    let safety = 0;

    while (hasMore && safety < 6) {
      const input = {
        sessionId,
        afterMessageId,
        limit: SESSION_MESSAGE_PAGE_SIZE,
      };
      let updates;

      try {
        updates = await loadChatSessionMessageUpdates(input, messageStore.value);
      } catch (error) {
        hadDesktopRefreshFailure = true;
        if (sessionMessageUpdatesFailedSessionId !== sessionId) {
          showTransportNotice({
            id: `load-session-message-updates-failed-${sessionId}-${Date.now()}`,
            tone: "warn",
            title: "Live message refresh failed",
            detail: describeCommandError(
              error,
              "Desktop live message updates could not be refreshed, so the UI kept using the locally cached message list.",
            ),
          });
        }

        sessionMessageUpdatesFailedSessionId = sessionId;
        updates = loadChatSessionMessageUpdatesLocally(input, messageStore.value);
      }

      if (!updates.messages.length) {
        break;
      }

      mergedMessages = mergeLoadedSessionMessages(mergedMessages, updates.messages);
      afterMessageId = updates.messages[updates.messages.length - 1]?.id ?? afterMessageId;
      hasMore = updates.hasMore;
      changed = true;
      safety += 1;
    }

    if (!hadDesktopRefreshFailure && sessionMessageUpdatesFailedSessionId === sessionId) {
      sessionMessageUpdatesFailedSessionId = "";
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
        subtitle: sessionSubtitleFromMessage(sessionId, lastMessage),
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

    const input = {
      sessionId,
      beforeMessageId: currentPage.nextBeforeMessageId,
      limit: SESSION_MESSAGE_PAGE_SIZE,
    };
    let page;

    try {
      page = await loadChatSessionMessages(input, messageStore.value);
    } catch (error) {
      showTransportNotice({
        id: `load-older-messages-failed-${sessionId}-${Date.now()}`,
        tone: "warn",
        title: "Older messages load failed",
        detail: describeCommandError(
          error,
          "Desktop history pagination could not read older messages, so the UI fell back to the locally cached message list.",
        ),
      });
      page = loadChatSessionMessagesLocally(input, messageStore.value);
    }

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
      const hydratedMessages = hydrateSessionMessages(sessionId, messages);
      if (messageStoreMode === "preview") {
        const normalized = buildSessionMessagePageState(hydratedMessages);
        nextMessageStore[sessionId] = normalized.messages;
        nextSessionMessagePages[sessionId] = normalized.page;
        continue;
      }

      nextMessageStore[sessionId] = hydratedMessages;
      nextSessionMessagePages[sessionId] = buildLoadedSessionMessageState();
    }

    messageStore.value = nextMessageStore;
    sessionMessagePages.value = nextSessionMessagePages;
  }

  function applyShellSnapshot(state: ShellStateSnapshot) {
    isAuthenticated.value = state.isAuthenticated;
    authSession.value = state.authSession;
    authRuntime.value = state.authRuntime;
    authRuntimeBinding.value = state.authRuntimeBinding;
    userProfile.value = state.userProfile;
    restorableCircles.value = state.restorableCircles;
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

  async function refreshDomainOverview(options: { showFailureNotice?: boolean } = {}) {
    const currentOverview: ChatDomainOverview = {
      circles: circles.value,
      contacts: contacts.value,
      sessions: sessions.value,
      groups: groups.value,
    };
    let nextOverview: ChatDomainOverview;

    try {
      nextOverview = await loadChatDomainOverview(currentOverview);
      domainOverviewBackgroundRefreshFailed = false;
    } catch (error) {
      if (options.showFailureNotice || !domainOverviewBackgroundRefreshFailed) {
        showTransportNotice({
          id: `load-domain-overview-failed-${Date.now()}`,
          tone: "warn",
          title: "Chat overview refresh failed",
          detail: describeCommandError(
            error,
            "Desktop chat overview could not be refreshed, so the UI kept using the locally cached overview.",
          ),
        });
      }

      domainOverviewBackgroundRefreshFailed = true;
      nextOverview = loadChatDomainOverviewLocally(currentOverview);
    }

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

  function navigateMentionSuggestions(direction: 1 | -1) {
    if (!showMentionSuggestions.value) {
      return;
    }

    const total = mentionSuggestions.value.length;
    if (!total) {
      return;
    }

    mentionSelectionIndex.value =
      (mentionSelectionIndex.value + direction + total) % total;
  }

  function selectMentionSuggestion(contactId?: string) {
    const match = activeComposerMentionMatch.value;
    if (!match) {
      return false;
    }

    const selectedContact =
      mentionSuggestions.value.find((contact) => contact.id === contactId) ??
      mentionSuggestions.value[mentionSelectionIndex.value];
    if (!selectedContact) {
      return false;
    }

    const nextValue =
      `${composerText.value.slice(0, match.mentionStart)}${selectedContact.handle} `;
    updateComposerText(nextValue);
    mentionSelectionIndex.value = 0;
    return true;
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

  function resetFindPeopleRequestState() {
    findPeopleSubmitting.value = false;
    findPeopleErrorMessage.value = "";
  }

  function setFindPeopleErrorMessage(message: string) {
    findPeopleErrorMessage.value = message.trim();
  }

  async function chooseCircle(circleId: string) {
    showCircleSwitcher.value = false;
    await landOnCircle(circleId);
  }

  function focusCircle(
    circleId: string,
    options: {
      nextOverlay?: "new-message";
    } = {},
  ) {
    if (!circles.value.some((circle) => circle.id === circleId)) {
      return;
    }

    activeCircleId.value = circleId;
    searchText.value = "";
    closeTransientChrome();

    if (options.nextOverlay === "new-message") {
      applyOverlayPages([{ kind: "new-message" }]);
      return;
    }

    closeAllOverlayPages();
  }

  function toggleCircleSwitcher() {
    showCircleSwitcher.value = !showCircleSwitcher.value;
  }

  function openNewMessage() {
    if (!activeCircle.value) {
      openFindPeoplePage("join-circle");
      return;
    }

    pushOverlayPage({ kind: "new-message" });
  }

  function openCircleInvitePage() {
    if (!activeCircle.value) {
      openFindPeoplePage("join-circle");
      return;
    }

    pushOverlayPage({ kind: "circle-invite" });
  }

  function openSelfChatConfirmPage() {
    if (!activeCircle.value) {
      openFindPeoplePage("join-circle");
      return;
    }

    pushOverlayPage({ kind: "self-chat-confirm" });
  }

  function openFindPeoplePage(mode: "chat" | "join-circle" = "chat") {
    resetFindPeopleRequestState();
    pushOverlayPage({
      kind: "find-people",
      mode: !activeCircle.value && mode === "chat" ? "join-circle" : mode,
    });
  }

  function redirectToCircleSetup(detail = "Add or restore a circle before starting chats.") {
    showTransportNotice({
      id: "missing-active-circle",
      tone: "warn",
      title: "No active circle",
      detail,
    });

    if (activeOverlayPage.value?.kind === "find-people" && activeOverlayPage.value.mode === "join-circle") {
      return;
    }

    if (activeOverlayPage.value) {
      replaceTopOverlayPage({ kind: "find-people", mode: "join-circle" });
      return;
    }

    pushOverlayPage({ kind: "find-people", mode: "join-circle" });
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

  function openMessageDetailPage(messageId: string, sessionId = selectedSession.value?.id) {
    if (!sessionId || !advancedPreferences.value.showMessageInfo) {
      return;
    }

    pushOverlayPage({ kind: "message-detail", sessionId, messageId });
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
    if (!activeCircle.value) {
      openFindPeoplePage("join-circle");
      return;
    }

    pushOverlayPage({ kind: "group-select-members" });
  }

  function openGroupCreatePage(memberContactIds: string[]) {
    if (!activeCircle.value) {
      openFindPeoplePage("join-circle");
      return;
    }

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

    if (selectedSession.value.kind === "self") {
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
        pubkey:
          input.access.kind === "npub" && input.access.value?.trim()
            ? input.access.value.trim()
            : undefined,
      },
      circleSelectionMode: input.circleSelection.mode,
      loggedInAt: input.loggedInAt ?? new Date().toISOString(),
    };
  }

  function buildAuthRuntimeSummary(
    input: LoginCompletionInput,
    sessionSummary: AuthSessionSummary = buildAuthSessionSummary(input),
  ): AuthRuntimeSummary {
    return (
      deriveAuthRuntimeFromAuthSession(sessionSummary) ?? {
        state: "failed",
        loginMethod: input.method,
        accessKind: input.access.kind,
        label: sessionSummary.access.label,
        pubkey: sessionSummary.access.pubkey,
        error: "Unable to derive auth runtime state from the current session summary.",
        canSendMessages: false,
        sendBlockedReason: "Unable to derive auth runtime state from the current session summary.",
        persistedInNativeStore: false,
        credentialPersistedInNativeStore: false,
        updatedAt: sessionSummary.loggedInAt,
      }
    );
  }

  function buildLoggedOutShellSnapshot(): ChatShellSnapshot {
    const loggedOutState = createLoggedOutShellState({
      restorableCircles: cloneState(restorableCircles.value),
      appPreferences: cloneState(appPreferences.value),
      notificationPreferences: cloneState(notificationPreferences.value),
      advancedPreferences: cloneState(advancedPreferences.value),
    });

    return createChatShellSnapshotFromPersistedState(loggedOutState);
  }

  function buildRestorableCircleEntry(circle: {
    name: string;
    relay: string;
    type: RestorableCircleEntry["type"];
    description: string;
  }): RestorableCircleEntry {
    return {
      name: circle.name.trim() || "Restored Circle",
      relay: circle.relay.trim(),
      type: circle.type,
      description: circle.description.trim(),
      archivedAt: new Date().toISOString(),
    };
  }

  function sameRelay(left: string, right: string) {
    return left.trim().toLowerCase() === right.trim().toLowerCase();
  }

  function upsertRestorableCircle(entry: RestorableCircleEntry) {
    restorableCircles.value = [
      cloneState(entry),
      ...restorableCircles.value.filter((item) => !sameRelay(item.relay, entry.relay)),
    ];
  }

  function forgetRestorableCircle(relay: string) {
    restorableCircles.value = restorableCircles.value.filter((entry) => {
      return !sameRelay(entry.relay, relay);
    });
  }

  function forgetRestorableCircleByCircleId(circleId: string) {
    const circle = circles.value.find((item) => item.id === circleId);
    if (!circle) {
      return;
    }

    forgetRestorableCircle(circle.relay);
  }

  function applyLocalRestoreCircle(entry: RestorableCircleEntry) {
    const existing = circles.value.find((circle) => sameRelay(circle.relay, entry.relay));
    if (existing) {
      return existing.id;
    }

    const nextCircle = {
      id: buildUniqueCircleId(entry.name),
      name: entry.name.trim() || "Restored Circle",
      relay: entry.relay.trim(),
      type: entry.type,
      status: "connecting",
      latency: "--",
      description: entry.description.trim(),
    } as const;

    circles.value = [nextCircle, ...circles.value];
    return nextCircle.id;
  }

  async function restoreRestorableCircle(entry: RestorableCircleEntry) {
    const mutation = await runFallbackEligibleMutation(() => restoreChatCircle({
      name: entry.name,
      relay: entry.relay,
      type: entry.type,
      description: entry.description,
    }), {
      title: "Restore circle failed",
      fallbackDetail: "Desktop restore could not rehydrate the native circle entry.",
    });

    if (mutation.result) {
      applyDomainSeed(mutation.result.seed, {
        preferredCircleId: mutation.result.circleId,
        preferredSessionId: selectedSessionId.value,
      });
      forgetRestorableCircleByCircleId(mutation.result.circleId);
      return mutation.result.circleId;
    }

    if (!mutation.canFallbackLocally) {
      return null;
    }

    const circleId = applyLocalRestoreCircle(entry);
    if (circleId) {
      forgetRestorableCircle(entry.relay);
    }
    return circleId;
  }

  async function restoreCircleAccess(entry: RestorableCircleEntry) {
    const circleId = await restoreRestorableCircle(entry);
    if (circleId) {
      await landOnCircle(circleId);
    }
  }

  async function resolveLoginCircle(selection: LoginCircleSelectionInput): Promise<{
    circleId: string;
    canFallbackToExistingCircle: boolean;
    openJoinCirclePicker: boolean;
  }> {
    if (selection.mode === "restore") {
      const selectedCatalogEntries =
        selection.relays?.length
          ? selection.relays
              .map((relay) => {
                return restorableCircles.value.find((entry) => sameRelay(entry.relay, relay)) ?? null;
              })
              .filter((entry): entry is RestorableCircleEntry => !!entry)
              .filter((entry, index, entries) => {
                return entries.findIndex((candidate) => sameRelay(candidate.relay, entry.relay)) === index;
              })
          : [
              restorableCircles.value.find((entry) => {
                if (!selection.relay?.trim()) {
                  return false;
                }

                return sameRelay(entry.relay, selection.relay);
              }) ?? restorableCircles.value[0],
            ].filter((entry): entry is RestorableCircleEntry => !!entry);
      if (selectedCatalogEntries.length) {
        let primaryCircleId = "";
        let restoredAnyCircle = false;

        for (const entry of selectedCatalogEntries) {
          const circleId = await restoreRestorableCircle(entry);
          if (circleId && !primaryCircleId) {
            primaryCircleId = circleId;
          }
          restoredAnyCircle = restoredAnyCircle || !!circleId;
        }

        return {
          circleId: primaryCircleId,
          canFallbackToExistingCircle: restoredAnyCircle,
          openJoinCirclePicker: false,
        };
      }

      return {
        circleId: circles.value[0]?.id ?? activeCircleId.value,
        canFallbackToExistingCircle: true,
        openJoinCirclePicker: false,
      };
    }

    if (selection.mode === "existing") {
      const preferredCircleId = selection.circleId?.trim() ?? "";
      return {
        circleId:
          circles.value.find((circle) => circle.id === preferredCircleId)?.id ??
          circles.value[0]?.id ??
          activeCircleId.value,
        canFallbackToExistingCircle: true,
        openJoinCirclePicker: false,
      };
    }

    if (selection.mode === "invite" && !selection.inviteCode?.trim()) {
      return {
        circleId: "",
        canFallbackToExistingCircle: false,
        openJoinCirclePicker: true,
      };
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

    const mutation = await runFallbackEligibleMutation(() => addChatCircle(addInput), {
      title: "Circle setup failed",
      fallbackDetail: "Desktop login could not persist the selected circle in native state.",
    });
    if (mutation.result) {
      applyDomainSeed(mutation.result.seed, {
        preferredCircleId: mutation.result.circleId,
        preferredSessionId: selectedSessionId.value,
      });
      forgetRestorableCircleByCircleId(mutation.result.circleId);
      return {
        circleId: mutation.result.circleId,
        canFallbackToExistingCircle: true,
        openJoinCirclePicker: false,
      };
    }

    if (!mutation.canFallbackLocally) {
      return {
        circleId: "",
        canFallbackToExistingCircle: false,
        openJoinCirclePicker: false,
      };
    }

    const circleId = applyLocalAddCircleFromDirectory(addInput);
    forgetRestorableCircleByCircleId(circleId);
    return {
      circleId,
      canFallbackToExistingCircle: true,
      openJoinCirclePicker: false,
    };
  }

  async function completeLogin(input: LoginCompletionInput) {
    const nextInput = cloneState({
      ...input,
      loggedInAt: input.loggedInAt ?? new Date().toISOString(),
    }) as LoginCompletionInput;
    const wantsPostLoginJoinCirclePicker =
      nextInput.circleSelection.mode === "invite" &&
      !nextInput.circleSelection.inviteCode?.trim();
    let completedSnapshot: ChatShellSnapshot | null = null;

    if (!wantsPostLoginJoinCirclePicker) {
      try {
        completedSnapshot = await completeLoginViaRuntime(nextInput);
      } catch (error) {
        showTransportNotice({
          id: `complete-login-failed-${Date.now()}`,
          tone: "warn",
          title: "Login failed",
          detail: describeCommandError(
            error,
            "Desktop login could not finish the native shell bootstrap.",
          ),
        });
        return;
      }
    }

    if (completedSnapshot) {
      resetDesktopFailureFlags();
      closeTransientChrome();
      closeAllOverlayPages();
      applyChatShellSnapshot(completedSnapshot);
      if (activeCircleId.value) {
        await ensureCircleHasSendableSession(activeCircleId.value);
      }
      searchText.value = "";
      composerText.value = "";
      return;
    }

    let bootstrappedShell: ShellStateSnapshot | null = null;

    try {
      bootstrappedShell = await bootstrapAuthSession(nextInput);
    } catch (error) {
      showTransportNotice({
        id: `bootstrap-login-failed-${Date.now()}`,
        tone: "warn",
        title: "Login bootstrap failed",
        detail: describeCommandError(
          error,
          "Desktop login could not persist the auth session bootstrap state.",
        ),
      });
      return;
    }

    if (bootstrappedShell) {
      resetDesktopFailureFlags();
      applyShellSnapshot(bootstrappedShell);
      isAuthenticated.value = true;
    } else {
      const nextAuthSession = buildAuthSessionSummary(nextInput);
      resetDesktopFailureFlags();
      isAuthenticated.value = true;
      authSession.value = nextAuthSession;
      authRuntime.value = buildAuthRuntimeSummary(nextInput, nextAuthSession);
      authRuntimeBinding.value = buildAuthRuntimeBindingSummary(nextInput, false);
      userProfile.value = cloneState(nextInput.userProfile) as UserProfile;
    }

    dismissTransportNotice();
    closeTransientChrome();
    closeAllOverlayPages();
    await refreshDomainOverview({ showFailureNotice: true });

    if (wantsPostLoginJoinCirclePicker) {
      activeCircleId.value = "";
      selectedSessionId.value = "";
      openFindPeoplePage("join-circle");
      return;
    }

    const circleResolution = await resolveLoginCircle(nextInput.circleSelection);

    if (circleResolution.openJoinCirclePicker) {
      activeCircleId.value = "";
      selectedSessionId.value = "";
      openFindPeoplePage("join-circle");
      return;
    }

    if (circleResolution.circleId) {
      await landOnCircle(circleResolution.circleId);
      return;
    }

    if (circleResolution.canFallbackToExistingCircle && circles.value[0]?.id) {
      await landOnCircle(circles.value[0].id);
      return;
    }

    activeCircleId.value = "";
    selectedSessionId.value = "";
    showTransportNotice({
      id: `login-without-circle-${Date.now()}`,
      tone: "warn",
      title: "Logged in without a circle",
      detail:
        "Desktop auth completed, but the selected circle could not be restored or added. Add or restore a circle before starting chats.",
    });
  }

  async function logout() {
    const snapshot = buildLoggedOutShellSnapshot();
    let runtimeSnapshot: ChatShellSnapshot | null = null;

    try {
      runtimeSnapshot = await logoutChatSession();
    } catch (error) {
      showTransportNotice({
        id: `logout-failed-${Date.now()}`,
        tone: "warn",
        title: "Logout failed",
        detail: describeCommandError(
          error,
          "Desktop logout did not clear the native shell state. Try again before closing the app.",
        ),
      });
      return;
    }

    cancelPendingDraftPersistence();
    clearAuthRuntimeSyncTimer();
    resetDesktopFailureFlags();
    if (persistTimer) {
      window.clearTimeout(persistTimer);
      persistTimer = null;
    }

    dismissTransportNotice();
    transportBusyCircleId.value = "";
    transportSnapshot.value = null;
    closeTransientChrome();
    closeAllOverlayPages();
    composerText.value = "";
    searchText.value = "";

    if (runtimeSnapshot) {
      applyChatShellSnapshot(runtimeSnapshot);
      persistChatShellSnapshotLocally(runtimeSnapshot);
      return;
    }

    applyChatShellSnapshot(snapshot);
    persistChatShellSnapshotLocally(snapshot);
    void persistShellSnapshotToNative(snapshot);
  }

  async function updateAuthRuntime(input: UpdateAuthRuntimeInput) {
    const nextInput = cloneState({
      ...input,
      updatedAt: input.updatedAt ?? new Date().toISOString(),
    }) as UpdateAuthRuntimeInput;
    let runtimeShell: ShellStateSnapshot | null = null;

    try {
      runtimeShell = await updateAuthRuntimeViaRuntime(nextInput);
    } catch (error) {
      showTransportNotice({
        id: `update-auth-runtime-failed-${Date.now()}`,
        tone: "warn",
        title: "Auth runtime update failed",
        detail: describeCommandError(
          error,
          "Desktop auth runtime state could not be updated in native storage.",
        ),
      });
      return;
    }

    if (runtimeShell) {
      applyShellSnapshot(runtimeShell);
      persistChatShellSnapshotLocally(snapshotChatShellState());
      return;
    }

    const nextRuntime = buildUpdatedAuthRuntime(
      authSession.value,
      effectiveAuthRuntime.value,
      nextInput,
    );
    if (!nextRuntime) {
      return;
    }

    authRuntime.value = nextRuntime;
    schedulePersistence();
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

  function contactById(contactId?: string | null) {
    const normalizedContactId = contactId?.trim() ?? "";
    if (!normalizedContactId) {
      return null;
    }

    return contacts.value.find((contact) => contact.id === normalizedContactId) ?? null;
  }

  function resolveMessageAuthorContact(session: SessionItem | null, message: MessageItem) {
    if (!session || message.author !== "peer") {
      return null;
    }

    const existingAuthorContact = contactById(message.authorContactId);
    if (existingAuthorContact) {
      return existingAuthorContact;
    }

    if (session.kind === "direct") {
      return contactById(session.contactId);
    }

    if (session.kind !== "group") {
      return null;
    }

    const group = groups.value.find((candidate) => candidate.sessionId === session.id) ?? null;
    const groupMemberIds = new Set(group?.members.map((member) => member.contactId) ?? []);
    if (!groupMemberIds.size) {
      return null;
    }

    const senderPubkey = normalizeNostrPubkey(message.signedNostrEvent?.pubkey);
    if (!senderPubkey) {
      return null;
    }

    return (
      contacts.value.find((contact) => {
        return groupMemberIds.has(contact.id) && normalizeNostrPubkey(contact.pubkey) === senderPubkey;
      }) ?? null
    );
  }

  function hydrateMessageAuthor(sessionId: string, message: MessageItem) {
    const session = sessions.value.find((candidate) => candidate.id === sessionId) ?? null;
    if (!session || message.author === "system") {
      return message;
    }

    if (message.author === "me") {
      const nextAuthorName = userProfile.value.name.trim() || undefined;
      const nextAuthorInitials =
        userProfile.value.initials.trim() || buildInitials(nextAuthorName ?? message.authorName ?? "You");
      if (message.authorName === nextAuthorName && message.authorInitials === nextAuthorInitials) {
        return message;
      }

      return {
        ...message,
        authorName: nextAuthorName,
        authorInitials: nextAuthorInitials,
      };
    }

    const authorContact = resolveMessageAuthorContact(session, message);
    const nextAuthorName =
      authorContact?.name ??
      message.authorName?.trim() ??
      (session.kind === "direct" ? session.name : undefined);
    const nextAuthorInitials =
      authorContact?.initials ??
      message.authorInitials?.trim() ??
      (session.kind === "direct" ? session.initials : undefined);
    const nextAuthorContactId =
      authorContact?.id ??
      message.authorContactId?.trim() ??
      (session.kind === "direct" ? session.contactId : undefined);

    if (
      message.authorName === nextAuthorName &&
      message.authorInitials === nextAuthorInitials &&
      message.authorContactId === nextAuthorContactId
    ) {
      return message;
    }

    return {
      ...message,
      authorName: nextAuthorName,
      authorInitials: nextAuthorInitials,
      authorContactId: nextAuthorContactId,
    };
  }

  function registerMessageReference(index: Map<string, MessageItem>, message: MessageItem) {
    index.set(message.id, message);
    if (message.remoteId) {
      index.set(message.remoteId, message);
    }

    const signedEventId = message.signedNostrEvent?.eventId;
    if (signedEventId) {
      index.set(signedEventId, message);
    }
  }

  function hydrateSessionMessages(sessionId: string, messages: MessageItem[]) {
    const session = sessions.value.find((candidate) => candidate.id === sessionId) ?? null;
    if (!session) {
      return messages;
    }

    const hydratedMessages = messages.map((message) => hydrateMessageAuthor(sessionId, message));
    const messageIndex = new Map<string, MessageItem>();

    for (const message of messageStore.value[sessionId] ?? []) {
      registerMessageReference(messageIndex, hydrateMessageAuthor(sessionId, message));
    }
    for (const message of hydratedMessages) {
      registerMessageReference(messageIndex, message);
    }

    return hydratedMessages.map((message) => {
      if (!message.replyTo) {
        return message;
      }

      const replyTarget =
        messageIndex.get(message.replyTo.messageId) ??
        (message.replyTo.remoteId ? messageIndex.get(message.replyTo.remoteId) : undefined);
      const nextAuthorLabel = replyTarget
        ? resolveMessageAuthorLabel(session, replyTarget)
        : resolveReplyPreviewAuthorLabel(session, message.replyTo);
      if (message.replyTo.authorLabel === nextAuthorLabel) {
        return message;
      }

      return {
        ...message,
        replyTo: {
          ...message.replyTo,
          authorLabel: nextAuthorLabel,
        },
      };
    });
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
    if (classifyChatQuery(trimmed) === "relay") {
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

  function shouldReturnToNewMessageAfterCircleJoin() {
    const activePage = overlayPages.value[overlayPages.value.length - 1];
    const previousPage = overlayPages.value[overlayPages.value.length - 2];

    return activePage?.kind === "find-people" && previousPage?.kind === "new-message";
  }

  function messagePreviewSnippet(message: MessageItem) {
    switch (message.kind) {
      case "image":
        return `Shared image: ${message.body}`;
      case "video":
        return `Shared video: ${message.body}`;
      case "file":
        return `Shared file: ${message.body}`;
      case "audio":
        return `Audio: ${message.meta ?? "Voice note"}`;
      default:
        return message.body || "Empty message";
    }
  }

  function messageReplyAuthorLabel(message: MessageItem) {
    return resolveMessageAuthorLabel(selectedSession.value, message);
  }

  function buildLocalReplyPreview(message: MessageItem) {
    return {
      messageId: message.id,
      remoteId: message.remoteId,
      author: message.author,
      authorLabel: messageReplyAuthorLabel(message),
      kind: message.kind,
      snippet: messagePreviewSnippet(message),
    };
  }

  function startReplyToMessage(messageId: string) {
    if (!selectedSession.value) {
      return;
    }

    const message = (messageStore.value[selectedSession.value.id] ?? []).find(
      (item) => item.id === messageId,
    );
    if (!message || message.kind === "system") {
      return;
    }

    replyToMessageId.value = messageId;
  }

  function cancelReplyToMessage() {
    replyToMessageId.value = null;
  }

  function sessionMessageById(sessionId: string | null | undefined, messageId: string) {
    const normalizedSessionId = sessionId?.trim() ?? "";
    if (!normalizedSessionId) {
      return null;
    }

    const session = sessions.value.find((candidate) => candidate.id === normalizedSessionId) ?? null;
    if (!session) {
      return null;
    }

    const message = (messageStore.value[normalizedSessionId] ?? []).find((item) => item.id === messageId) ?? null;
    return message ? { session, message } : null;
  }

  function selectedSessionMessageById(messageId: string) {
    return sessionMessageById(selectedSession.value?.id, messageId);
  }

  function messageAttachmentLocalPath(message: MessageItem) {
    switch (message.kind) {
      case "file":
        return fileMessageLocalPath(message);
      case "image":
        return imageMessageLocalPath(message);
      case "video":
        return videoMessageLocalPath(message);
      default:
        return "";
    }
  }

  function messageAttachmentRemoteUrl(message: MessageItem) {
    switch (message.kind) {
      case "file":
        return fileMessageRemoteUrl(message);
      case "image":
        return imageMessageRemoteUrl(message);
      case "video":
        return videoMessageRemoteUrl(message);
      default:
        return "";
    }
  }

  async function ensureMessageAttachmentLocalPath(messageId: string, sessionId?: string) {
    const selected = sessionMessageById(sessionId ?? selectedSession.value?.id, messageId);
    if (!selected) {
      return "";
    }

    const existingLocalPath = messageAttachmentLocalPath(selected.message);
    if (existingLocalPath) {
      return existingLocalPath;
    }

    const remoteUrl = messageAttachmentRemoteUrl(selected.message);
    if (!remoteUrl) {
      return "";
    }

    try {
      const result = await cacheChatMessageMedia({
        sessionId: selected.session.id,
        messageId: selected.message.id,
      });
      if (!result) {
        return "";
      }

      applyDomainSeed(result.seed, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: selectedSession.value?.id ?? selected.session.id,
      });
      return result.localPath;
    } catch (error) {
      showTransportNotice({
        id: `cache-attachment-failed-${selected.session.id}-${selected.message.id}`,
        tone: "warn",
        title: "Attachment cache failed",
        detail: describeCommandError(
          error,
          `The remote attachment for message ${selected.message.id} could not be cached locally.`,
        ),
        circleId: selected.session.circleId,
      });
      return "";
    }
  }

  function messageAttachmentLabel(message: MessageItem) {
    switch (message.kind) {
      case "file":
        return "attachment";
      case "image":
        return "image";
      case "video":
        return "video";
      default:
        return "media";
    }
  }

  function clipboardMessageContent(message: MessageItem) {
    switch (message.kind) {
      case "image":
        return imageMessageMetaLabel(message)
          ? `${message.body}\n${imageMessageMetaLabel(message)}`
          : message.body;
      case "video":
        return videoMessageMetaLabel(message)
          ? `${message.body}\n${videoMessageMetaLabel(message)}`
          : message.body;
      case "file":
        return fileMessageMetaLabel(message)
          ? `${message.body}\n${fileMessageMetaLabel(message)}`
          : message.body;
      case "audio":
        return message.meta ?? message.body;
      default:
        return message.body;
    }
  }

  function clipboardCopyLabel(message: MessageItem) {
    switch (message.kind) {
      case "image":
        return "Image name";
      case "video":
        return "Video name";
      case "file":
        return "File name";
      case "audio":
        return "Audio label";
      default:
        return "Message";
    }
  }

  async function writeClipboardText(value: string) {
    if (typeof navigator === "undefined" || !navigator.clipboard?.writeText) {
      throw new Error("Clipboard unavailable");
    }

    await navigator.clipboard.writeText(value);
  }

  async function copyMessageContent(messageId: string) {
    const selected = selectedSessionMessageById(messageId);
    if (!selected) {
      return;
    }

    try {
      await writeClipboardText(clipboardMessageContent(selected.message));
      showTransportNotice({
        id: `copy-message-${selected.session.id}-${selected.message.id}`,
        tone: "info",
        title: `${clipboardCopyLabel(selected.message)} copied`,
        detail: `The selected ${selected.message.kind} content is now on your clipboard.`,
        circleId: selected.session.circleId,
      });
    } catch (error) {
      showTransportNotice({
        id: `copy-message-failed-${selected.session.id}-${selected.message.id}`,
        tone: "warn",
        title: "Copy failed",
        detail: describeCommandError(
          error,
          `Clipboard write did not complete for message ${selected.message.id}.`,
        ),
        circleId: selected.session.circleId,
      });
    }
  }

  async function reportMessage(messageId: string, reason: string) {
    const selected = selectedSessionMessageById(messageId);
    if (!selected || selected.message.author !== "peer") {
      return;
    }

    const reportPackage = {
      reportedAt: new Date().toISOString(),
      reason,
      sessionId: selected.session.id,
      sessionName: selected.session.name,
      circleId: selected.session.circleId,
      messageId: selected.message.id,
      remoteId: selected.message.remoteId ?? null,
      author: selected.message.author,
      kind: selected.message.kind,
      body: selected.message.body,
      meta: selected.message.meta ?? null,
      localAttachmentPath: messageAttachmentLocalPath(selected.message) || null,
      remoteAttachmentUrl: messageAttachmentRemoteUrl(selected.message) || null,
      syncSource: selected.message.syncSource ?? null,
      ackedAt: selected.message.ackedAt ?? null,
      signedNostrEvent: selected.message.signedNostrEvent ?? null,
      replyTo: selected.message.replyTo ?? null,
    };

    try {
      await writeClipboardText(JSON.stringify(reportPackage, null, 2));
      showTransportNotice({
        id: `report-message-${selected.session.id}-${selected.message.id}-${reason}`,
        tone: "info",
        title: "Report package copied",
        detail:
          "A moderation handoff JSON package was copied to your clipboard. Remote report submission is not wired yet.",
        circleId: selected.session.circleId,
      });
    } catch (error) {
      showTransportNotice({
        id: `report-message-failed-${selected.session.id}-${selected.message.id}`,
        tone: "warn",
        title: "Report copy failed",
        detail: describeCommandError(
          error,
          "The report package could not be copied to clipboard.",
        ),
        circleId: selected.session.circleId,
      });
    }
  }

  async function openMessageAttachment(messageId: string, sessionId?: string) {
    const selected = sessionMessageById(sessionId ?? selectedSession.value?.id, messageId);
    if (!selected) {
      return;
    }

    const localPath = await ensureMessageAttachmentLocalPath(messageId, selected.session.id);
    if (!localPath) {
      showTransportNotice({
        id: `open-attachment-missing-${selected.session.id}-${selected.message.id}`,
        tone: "warn",
        title: "Attachment unavailable",
        detail: "This message does not have a local attachment path yet.",
        circleId: selected.session.circleId,
      });
      return;
    }

    try {
      await openPath(localPath);
      showTransportNotice({
        id: `open-attachment-${selected.session.id}-${selected.message.id}`,
        tone: "info",
        title: `${messageAttachmentLabel(selected.message)} opened`,
        detail: `Opened ${selected.message.body} using the system default application.`,
        circleId: selected.session.circleId,
      });
    } catch (error) {
      showTransportNotice({
        id: `open-attachment-failed-${selected.session.id}-${selected.message.id}`,
        tone: "warn",
        title: "Attachment open failed",
        detail: describeCommandError(
          error,
          `The local attachment for message ${selected.message.id} could not be opened.`,
        ),
        circleId: selected.session.circleId,
      });
    }
  }

  async function revealMessageAttachment(messageId: string, sessionId?: string) {
    const selected = sessionMessageById(sessionId ?? selectedSession.value?.id, messageId);
    if (!selected) {
      return;
    }

    const localPath = await ensureMessageAttachmentLocalPath(messageId, selected.session.id);
    if (!localPath) {
      showTransportNotice({
        id: `reveal-attachment-missing-${selected.session.id}-${selected.message.id}`,
        tone: "warn",
        title: "Attachment unavailable",
        detail: "This message does not have a local attachment path yet.",
        circleId: selected.session.circleId,
      });
      return;
    }

    try {
      await revealItemInDir(localPath);
      showTransportNotice({
        id: `reveal-attachment-${selected.session.id}-${selected.message.id}`,
        tone: "info",
        title: "Attachment revealed",
        detail: `Revealed ${selected.message.body} in the local file browser.`,
        circleId: selected.session.circleId,
      });
    } catch (error) {
      showTransportNotice({
        id: `reveal-attachment-failed-${selected.session.id}-${selected.message.id}`,
        tone: "warn",
        title: "Reveal in folder failed",
        detail: describeCommandError(
          error,
          `The local attachment for message ${selected.message.id} could not be revealed.`,
        ),
        circleId: selected.session.circleId,
      });
    }
  }

  async function copyMessageAttachmentPath(messageId: string, sessionId?: string) {
    const selected = sessionMessageById(sessionId ?? selectedSession.value?.id, messageId);
    if (!selected) {
      return;
    }

    const localPath = await ensureMessageAttachmentLocalPath(messageId, selected.session.id);
    if (!localPath) {
      showTransportNotice({
        id: `copy-attachment-path-missing-${selected.session.id}-${selected.message.id}`,
        tone: "warn",
        title: "Attachment path unavailable",
        detail: "This message does not have a local attachment path yet.",
        circleId: selected.session.circleId,
      });
      return;
    }

    try {
      await writeClipboardText(localPath);
      showTransportNotice({
        id: `copy-attachment-path-${selected.session.id}-${selected.message.id}`,
        tone: "info",
        title: "Attachment path copied",
        detail: `The local path for ${selected.message.body} is now on your clipboard.`,
        circleId: selected.session.circleId,
      });
    } catch (error) {
      showTransportNotice({
        id: `copy-attachment-path-failed-${selected.session.id}-${selected.message.id}`,
        tone: "warn",
        title: "Copy path failed",
        detail: describeCommandError(
          error,
          `Clipboard write did not complete for attachment path ${selected.message.id}.`,
        ),
        circleId: selected.session.circleId,
      });
    }
  }

  function localOutgoingSessionPreview(
    kind: MessageItem["kind"],
    body: string,
    meta?: string,
  ) {
    switch (kind) {
      case "image":
        return `Shared image: ${body}`;
      case "video":
        return `Shared video: ${body}`;
      case "file":
        return `Shared file: ${body}`;
      case "audio":
        return `Audio: ${meta ?? "Voice note"}`;
      default:
        return body;
    }
  }

  function formatAttachmentSize(bytes: number) {
    if (!Number.isFinite(bytes) || bytes <= 0) {
      return "Unknown size";
    }

    if (bytes >= 1024 * 1024 * 1024) {
      return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
    }

    if (bytes >= 1024 * 1024) {
      return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    }

    if (bytes >= 1024) {
      return `${Math.max(1, Math.round(bytes / 1024))} KB`;
    }

    return `${bytes} B`;
  }

  function formatAttachmentMeta(file: File) {
    const typeLabel = file.type.split("/")[1]
      ? file.type.split("/")[1].replace(/[-_]+/g, " ").toUpperCase()
      : "File";
    return `${typeLabel} · ${formatAttachmentSize(file.size)}`;
  }

  function formatImageAttachmentMeta(file: File, width?: number, height?: number) {
    const formatLabel = file.type.split("/")[1]
      ? file.type.split("/")[1].replace(/[-_]+/g, " ").toUpperCase()
      : "Image";
    const dimensionLabel = width && height ? `${width} x ${height}` : "Unknown size";
    return `${formatLabel} · ${dimensionLabel} · ${formatAttachmentSize(file.size)}`;
  }

  function formatVideoDuration(durationSeconds?: number) {
    if (!Number.isFinite(durationSeconds) || !durationSeconds || durationSeconds <= 0) {
      return "Unknown duration";
    }

    const totalSeconds = Math.max(1, Math.round(durationSeconds));
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    const seconds = totalSeconds % 60;

    if (hours > 0) {
      return `${hours}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
    }

    return `${minutes}:${String(seconds).padStart(2, "0")}`;
  }

  function formatVideoAttachmentMeta(
    file: File,
    durationSeconds?: number,
    width?: number,
    height?: number,
  ) {
    const formatLabel = file.type.split("/")[1]
      ? file.type.split("/")[1].replace(/[-_]+/g, " ").toUpperCase()
      : "Video";
    const dimensionLabel = width && height ? `${width} x ${height}` : "Unknown resolution";
    return `${formatLabel} · ${dimensionLabel} · ${formatVideoDuration(durationSeconds)} · ${formatAttachmentSize(file.size)}`;
  }

  async function persistNativeMediaAsset(
    input: StoreChatMediaAssetInput,
  ): Promise<StoredChatMediaAsset | null> {
    return storeChatMediaAsset(input);
  }

  async function cleanupOrphanedMediaAssets() {
    try {
      await cleanupChatMediaAssets();
    } catch {
      // Best-effort cleanup. Media GC failures should not block the main user action.
    }
  }

  function readFileAsDataUrl(file: File) {
    return new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => {
        if (typeof reader.result === "string" && reader.result) {
          resolve(reader.result);
          return;
        }

        reject(new Error("Could not read the selected file."));
      };
      reader.onerror = () => reject(reader.error ?? new Error("Could not read the selected file."));
      reader.readAsDataURL(file);
    });
  }

  function resolveImageDimensions(dataUrl: string) {
    return new Promise<{ width?: number; height?: number }>((resolve) => {
      const image = new Image();
      image.onload = () => resolve({ width: image.naturalWidth, height: image.naturalHeight });
      image.onerror = () => resolve({});
      image.src = dataUrl;
    });
  }

  function resolveVideoMetadata(dataUrl: string) {
    return new Promise<{ width?: number; height?: number; durationSeconds?: number }>((resolve) => {
      const video = document.createElement("video");
      video.preload = "metadata";
      video.onloadedmetadata = () =>
        resolve({
          width: video.videoWidth || undefined,
          height: video.videoHeight || undefined,
          durationSeconds: Number.isFinite(video.duration) ? video.duration : undefined,
        });
      video.onerror = () => resolve({});
      video.src = dataUrl;
    });
  }

  function applyLocalSendPreviewMessage(
    content: string,
    sessionId: string,
    replyToMessage: MessageItem | null,
  ) {
    const session = sessions.value.find((item) => item.id === sessionId);
    const message: MessageItem = {
      id: `local-${Date.now()}`,
      kind: "text",
      author: "me",
      body: content,
      time: "now",
      deliveryStatus: messageDeliveryStatusForCircle(session?.circleId ?? activeCircleId.value),
      syncSource: "local",
      replyTo: replyToMessage ? buildLocalReplyPreview(replyToMessage) : undefined,
    };
    const hydratedMessage = hydrateMessageAuthor(sessionId, message);

    messageStore.value[sessionId] = [...(messageStore.value[sessionId] ?? []), hydratedMessage];

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

  function applyLocalSendPreviewFileMessage(
    file: File,
    sessionId: string,
    replyToMessage: MessageItem | null,
  ) {
    const session = sessions.value.find((item) => item.id === sessionId);
    const normalizedName = file.name.trim() || "Untitled file";
    const meta = formatAttachmentMeta(file);
    const message: MessageItem = {
      id: `local-${Date.now()}`,
      kind: "file",
      author: "me",
      body: normalizedName,
      time: "now",
      meta,
      deliveryStatus: messageDeliveryStatusForCircle(session?.circleId ?? activeCircleId.value),
      syncSource: "local",
      replyTo: replyToMessage ? buildLocalReplyPreview(replyToMessage) : undefined,
    };
    const hydratedMessage = hydrateMessageAuthor(sessionId, message);

    messageStore.value[sessionId] = [...(messageStore.value[sessionId] ?? []), hydratedMessage];

    const targetIndex = sessions.value.findIndex((currentSession) => currentSession.id === sessionId);
    if (targetIndex < 0) {
      return;
    }

    const updatedSession = {
      ...sessions.value[targetIndex],
      subtitle: localOutgoingSessionPreview("file", normalizedName, meta),
      time: "now",
      draft: undefined,
    };

    const nextSessions = [...sessions.value];
    nextSessions.splice(targetIndex, 1);
    nextSessions.unshift(updatedSession);
    sessions.value = nextSessions;
  }

  function applyLocalSendPreviewImageMessage(
    input: SendImageMessageInput,
    sessionId: string,
    replyToMessage: MessageItem | null,
  ) {
    const session = sessions.value.find((item) => item.id === sessionId);
    const message: MessageItem = {
      id: `local-${Date.now()}`,
      kind: "image",
      author: "me",
      body: input.name,
      time: "now",
      meta: input.meta,
      deliveryStatus: messageDeliveryStatusForCircle(session?.circleId ?? activeCircleId.value),
      syncSource: "local",
      replyTo: replyToMessage ? buildLocalReplyPreview(replyToMessage) : undefined,
    };
    const hydratedMessage = hydrateMessageAuthor(sessionId, message);

    messageStore.value[sessionId] = [...(messageStore.value[sessionId] ?? []), hydratedMessage];

    const targetIndex = sessions.value.findIndex((currentSession) => currentSession.id === sessionId);
    if (targetIndex < 0) {
      return;
    }

    const updatedSession = {
      ...sessions.value[targetIndex],
      subtitle: localOutgoingSessionPreview("image", input.name),
      time: "now",
      draft: undefined,
    };

    const nextSessions = [...sessions.value];
    nextSessions.splice(targetIndex, 1);
    nextSessions.unshift(updatedSession);
    sessions.value = nextSessions;
  }

  function applyLocalSendPreviewVideoMessage(
    input: SendVideoMessageInput,
    sessionId: string,
    replyToMessage: MessageItem | null,
  ) {
    const session = sessions.value.find((item) => item.id === sessionId);
    const message: MessageItem = {
      id: `local-${Date.now()}`,
      kind: "video",
      author: "me",
      body: input.name,
      time: "now",
      meta: input.meta,
      deliveryStatus: messageDeliveryStatusForCircle(session?.circleId ?? activeCircleId.value),
      syncSource: "local",
      replyTo: replyToMessage ? buildLocalReplyPreview(replyToMessage) : undefined,
    };
    const hydratedMessage = hydrateMessageAuthor(sessionId, message);

    messageStore.value[sessionId] = [...(messageStore.value[sessionId] ?? []), hydratedMessage];

    const targetIndex = sessions.value.findIndex((currentSession) => currentSession.id === sessionId);
    if (targetIndex < 0) {
      return;
    }

    const updatedSession = {
      ...sessions.value[targetIndex],
      subtitle: localOutgoingSessionPreview("video", input.name),
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
    if (!content || !selectedSession.value || !canSendMessages.value) {
      return;
    }

    const sessionId = selectedSession.value.id;
    const currentReplyToMessage = replyingToMessage.value;
    const currentReplyToMessageId = currentReplyToMessage?.id;
    cancelPendingDraftPersistence();
    composerText.value = "";
    applyLocalSessionDraft(sessionId, "");
    let nextSeed: ChatDomainSeed | null = null;

    try {
      nextSeed = await sendChatMessage({
        sessionId,
        body: content,
        replyToMessageId: currentReplyToMessageId ?? undefined,
      });
    } catch (error) {
      composerText.value = content;
      applyLocalSessionDraft(sessionId, content);
      showTransportNotice({
        id: `send-message-failed-${sessionId}-${Date.now()}`,
        tone: "warn",
        title: "Send failed",
        detail: describeCommandError(
          error,
          "Desktop send did not complete. The composer text was restored and the runtime state will be refreshed.",
        ),
      });
      await refreshAuthRuntimeStateFromNative();
      return;
    }

    if (nextSeed) {
      replyToMessageId.value = null;
      applyDomainSeed(nextSeed, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: sessionId,
      });
      await refreshAuthRuntimeStateFromNative();
      return;
    }

    applyLocalSendPreviewMessage(content, sessionId, currentReplyToMessage);
    replyToMessageId.value = null;
  }

  async function sendAttachmentMessage(file: File) {
    if (!selectedSession.value || !canSendMessages.value) {
      return;
    }

    const sessionId = selectedSession.value.id;
    const normalizedName = file.name.trim();
    if (!normalizedName) {
      showTransportNotice({
        id: `send-file-empty-${sessionId}-${Date.now()}`,
        tone: "warn",
        title: "Attachment failed",
        detail: "The selected file does not have a usable name.",
      });
      return;
    }

    const currentReplyToMessage = replyingToMessage.value;
    const currentReplyToMessageId = currentReplyToMessage?.id;
    let nextSeed: ChatDomainSeed | null = null;

    if (file.type.startsWith("image/")) {
      let imageInput: SendImageMessageInput;
      try {
        const previewDataUrl = await readFileAsDataUrl(file);
        const dimensions = await resolveImageDimensions(previewDataUrl);
        const storedAsset = await persistNativeMediaAsset({
          kind: "image",
          name: normalizedName,
          dataUrl: previewDataUrl,
        });
        imageInput = {
          sessionId,
          name: normalizedName,
          meta: storedAsset
            ? encodeImageMessageMeta({
                label: formatImageAttachmentMeta(file, dimensions.width, dimensions.height),
                localPath: storedAsset.localPath,
              })
            : encodeImageMessageMeta({
                label: formatImageAttachmentMeta(file, dimensions.width, dimensions.height),
                localPath: previewDataUrl,
              }),
          replyToMessageId: currentReplyToMessageId ?? undefined,
        };
      } catch (error) {
        showTransportNotice({
          id: `send-image-read-failed-${sessionId}-${Date.now()}`,
          tone: "warn",
          title: "Image attachment failed",
          detail: describeCommandError(
            error,
            "The selected image could not be prepared for preview and persistence.",
          ),
        });
        return;
      }

      try {
        nextSeed = await sendChatImageMessage(imageInput);
      } catch (error) {
        await cleanupOrphanedMediaAssets();
        showTransportNotice({
          id: `send-image-failed-${sessionId}-${Date.now()}`,
          tone: "warn",
          title: "Image attachment failed",
          detail: describeCommandError(
            error,
            "Desktop image send did not complete. The runtime state will be refreshed before the next attempt.",
          ),
        });
        await refreshAuthRuntimeStateFromNative();
        return;
      }

      if (nextSeed) {
        replyToMessageId.value = null;
        applyDomainSeed(nextSeed, {
          preferredCircleId: activeCircleId.value,
          preferredSessionId: sessionId,
        });
        await refreshAuthRuntimeStateFromNative();
        return;
      }

      applyLocalSendPreviewImageMessage(imageInput, sessionId, currentReplyToMessage);
      replyToMessageId.value = null;
      return;
    }

    if (file.type.startsWith("video/")) {
      let videoInput: SendVideoMessageInput;
      try {
        const previewDataUrl = await readFileAsDataUrl(file);
        const metadata = await resolveVideoMetadata(previewDataUrl);
        const storedAsset = await persistNativeMediaAsset({
          kind: "video",
          name: normalizedName,
          dataUrl: previewDataUrl,
        });
        videoInput = {
          sessionId,
          name: normalizedName,
          meta: storedAsset
            ? encodeVideoMessageMeta({
                label: formatVideoAttachmentMeta(
                  file,
                  metadata.durationSeconds,
                  metadata.width,
                  metadata.height,
                ),
                localPath: storedAsset.localPath,
              })
            : encodeVideoMessageMeta({
                label: formatVideoAttachmentMeta(
                  file,
                  metadata.durationSeconds,
                  metadata.width,
                  metadata.height,
                ),
                localPath: previewDataUrl,
              }),
          replyToMessageId: currentReplyToMessageId ?? undefined,
        };
      } catch (error) {
        showTransportNotice({
          id: `send-video-read-failed-${sessionId}-${Date.now()}`,
          tone: "warn",
          title: "Video attachment failed",
          detail: describeCommandError(
            error,
            "The selected video could not be prepared for preview and persistence.",
          ),
        });
        return;
      }

      try {
        nextSeed = await sendChatVideoMessage(videoInput);
      } catch (error) {
        await cleanupOrphanedMediaAssets();
        showTransportNotice({
          id: `send-video-failed-${sessionId}-${Date.now()}`,
          tone: "warn",
          title: "Video attachment failed",
          detail: describeCommandError(
            error,
            "Desktop video send did not complete. The runtime state will be refreshed before the next attempt.",
          ),
        });
        await refreshAuthRuntimeStateFromNative();
        return;
      }

      if (nextSeed) {
        replyToMessageId.value = null;
        applyDomainSeed(nextSeed, {
          preferredCircleId: activeCircleId.value,
          preferredSessionId: sessionId,
        });
        await refreshAuthRuntimeStateFromNative();
        return;
      }

      applyLocalSendPreviewVideoMessage(videoInput, sessionId, currentReplyToMessage);
      replyToMessageId.value = null;
      return;
    }

    const label = formatAttachmentMeta(file);
    let meta = label;

    try {
      const dataUrl = await readFileAsDataUrl(file);
      const storedAsset = await persistNativeMediaAsset({
        kind: "file",
        name: normalizedName,
        dataUrl,
      });
      if (storedAsset) {
        meta = encodeFileMessageMeta({
          label,
          localPath: storedAsset.localPath,
        });
      }
    } catch (error) {
      showTransportNotice({
        id: `store-file-media-failed-${sessionId}-${Date.now()}`,
        tone: "warn",
        title: "Attachment storage failed",
        detail: describeCommandError(
          error,
          "The selected file could not be copied into the native media store.",
        ),
      });
      return;
    }

    try {
      nextSeed = await sendChatFileMessage({
        sessionId,
        name: normalizedName,
        meta,
        replyToMessageId: currentReplyToMessageId ?? undefined,
      });
    } catch (error) {
      await cleanupOrphanedMediaAssets();
      showTransportNotice({
        id: `send-file-failed-${sessionId}-${Date.now()}`,
        tone: "warn",
        title: "Attachment failed",
        detail: describeCommandError(
          error,
          "Desktop file send did not complete. The runtime state will be refreshed before the next attempt.",
        ),
      });
      await refreshAuthRuntimeStateFromNative();
      return;
    }

    if (nextSeed) {
      replyToMessageId.value = null;
      applyDomainSeed(nextSeed, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: sessionId,
      });
      await refreshAuthRuntimeStateFromNative();
      return;
    }

    applyLocalSendPreviewFileMessage(file, sessionId, currentReplyToMessage);
    replyToMessageId.value = null;
  }

  async function retryMessageDelivery(messageId: string) {
    if (!selectedSession.value || !canSendMessages.value) {
      return;
    }

    const sessionId = selectedSession.value.id;
    let nextSeed: ChatDomainSeed | null = null;

    try {
      nextSeed = await retryChatMessageDelivery({ sessionId, messageId });
    } catch (error) {
      showTransportNotice({
        id: `retry-message-failed-${sessionId}-${messageId}-${Date.now()}`,
        tone: "warn",
        title: "Retry failed",
        detail: describeCommandError(
          error,
          "Desktop retry did not complete. The runtime state will be refreshed before the next attempt.",
        ),
      });
      await refreshAuthRuntimeStateFromNative();
      return;
    }

    if (nextSeed) {
      applyDomainSeed(nextSeed, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: sessionId,
      });
      await refreshAuthRuntimeStateFromNative();
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
    messageStore.value[sessionId] = [];
    selectedSessionId.value = sessionId;

    return sessionId;
  }

  function applyLocalStartSelfConversation(circleId = activeCircleId.value) {
    if (!circleId) {
      return null;
    }

    const existing = sessions.value.find((session) => {
      return session.circleId === circleId && session.kind === "self";
    });

    if (existing) {
      if (existing.archived) {
        updateSession(existing.id, { archived: false });
      }

      selectSession(existing.id);
      return existing.id;
    }

    const sessionId = buildUniqueSessionId(`self-${circleId}`);
    const newSession: SessionItem = {
      id: sessionId,
      circleId,
      name: "File Transfer Assistant",
      initials: "ME",
      subtitle: "Add notes to yourself here.",
      time: "now",
      kind: "self",
      category: "system",
    };

    sessions.value = [newSession, ...sessions.value];
    messageStore.value[sessionId] = [];
    selectedSessionId.value = sessionId;

    return sessionId;
  }

  function firstVisibleSessionForCircle(circleId: string) {
    return (
      sessions.value.find((session) => {
        return session.circleId === circleId && !session.archived;
      }) ?? null
    );
  }

  async function openSelfConversationForCircle(
    circleId: string,
    options: {
      closeOverlays?: boolean;
    } = {},
  ) {
    if (!circleId || !circles.value.some((circle) => circle.id === circleId)) {
      return null;
    }

    const mutation = await runFallbackEligibleMutation(() => startSelfConversation({
      circleId,
    }), {
      title: "Open self chat failed",
      fallbackDetail: "Desktop chat state could not open the self-note session.",
    });

    if (mutation.result) {
      applyDomainSeed(mutation.result.seed, {
        preferredCircleId: circleId,
        preferredSessionId: mutation.result.sessionId,
      });
      if (options.closeOverlays) {
        closeAllOverlayPages();
      }
      return mutation.result.sessionId;
    }

    if (!mutation.canFallbackLocally) {
      return null;
    }

    const sessionId = applyLocalStartSelfConversation(circleId);
    if (sessionId && options.closeOverlays) {
      closeAllOverlayPages();
    }

    return sessionId;
  }

  async function ensureCircleHasSendableSession(circleId: string) {
    if (!circleId || !circles.value.some((circle) => circle.id === circleId)) {
      return null;
    }

    const existingSession = firstVisibleSessionForCircle(circleId);
    if (existingSession) {
      const selectedSessionStillVisible = sessions.value.some((session) => {
        return session.id === selectedSessionId.value && session.circleId === circleId && !session.archived;
      });
      if (!selectedSessionStillVisible) {
        selectSession(existingSession.id);
      }
      return existingSession.id;
    }

    return openSelfConversationForCircle(circleId);
  }

  async function landOnCircle(
    circleId: string,
    options: {
      nextOverlay?: "new-message";
    } = {},
  ) {
    if (!circleId) {
      return null;
    }

    focusCircle(circleId, options);
    if (options.nextOverlay === "new-message") {
      return null;
    }

    return ensureCircleHasSendableSession(circleId);
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
      redirectToCircleSetup();
      return;
    }

    const mutation = await runFallbackEligibleMutation(() => startDirectConversation({
      circleId: activeCircleId.value,
      contactId,
    }), {
      title: "Open conversation failed",
      fallbackDetail: "Desktop chat state could not create this direct conversation.",
    });

    if (mutation.result) {
      applyDomainSeed(mutation.result.seed, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: mutation.result.sessionId,
      });
      closeAllOverlayPages();
      return;
    }

    if (!mutation.canFallbackLocally) {
      return;
    }

    const sessionId = applyLocalStartConversation(contactId);
    if (sessionId) {
      closeAllOverlayPages();
    }
  }

  async function startSelfChat() {
    if (!activeCircleId.value) {
      redirectToCircleSetup();
      return;
    }

    await openSelfConversationForCircle(activeCircleId.value, { closeOverlays: true });
  }

  async function createGroupChat(input: Omit<CreateGroupConversationInput, "circleId">) {
    if (!activeCircleId.value) {
      redirectToCircleSetup();
      return;
    }

    const mutation = await runFallbackEligibleMutation(() => createGroupConversation({
      circleId: activeCircleId.value,
      name: input.name,
      memberContactIds: input.memberContactIds,
    }), {
      title: "Create group failed",
      fallbackDetail: "Desktop chat state could not create this group conversation.",
    });

    if (mutation.result) {
      applyDomainSeed(mutation.result.seed, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: mutation.result.sessionId,
      });
      closeAllOverlayPages();
      return;
    }

    if (!mutation.canFallbackLocally) {
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

  async function joinCircleFromLookup(
    query: string,
    options: {
      nextOverlay?: "new-message";
    } = {},
  ) {
    const normalizedQuery = query.trim();
    if (!normalizedQuery) {
      return false;
    }

    const input = inferCircleInputFromQuery(normalizedQuery);
    const mutation = await runFallbackEligibleMutation(() => addChatCircle(input), {
      title: "Join circle failed",
      fallbackDetail: "Desktop shell could not add this circle to native state.",
    });
    if (mutation.result) {
      applyDomainSeed(mutation.result.seed, {
        preferredCircleId: mutation.result.circleId,
        preferredSessionId: selectedSessionId.value,
      });
      forgetRestorableCircleByCircleId(mutation.result.circleId);
      await landOnCircle(mutation.result.circleId, options);
      return true;
    }

    if (!mutation.canFallbackLocally) {
      return false;
    }

    const circleId = applyLocalAddCircleFromDirectory(input);
    if (circleId) {
      forgetRestorableCircleByCircleId(circleId);
      await landOnCircle(circleId, options);
      return true;
    }

    return false;
  }

  async function updateGroupName(payload: UpdateGroupNameInput) {
    const nextName = payload.name.trim();
    if (!nextName) {
      return;
    }

    const mutation = await runFallbackEligibleMutation(() => updateChatGroupName({
      sessionId: payload.sessionId,
      name: nextName,
    }), {
      title: "Rename group failed",
      fallbackDetail: "Desktop chat state could not save the updated group name.",
    });

    if (mutation.result) {
      applyDomainSeed(mutation.result, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: payload.sessionId,
      });
      closeTopOverlayPage();
      return;
    }

    if (!mutation.canFallbackLocally) {
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

    const mutation = await runFallbackEligibleMutation(() => updateChatGroupMembers({
      sessionId: payload.sessionId,
      memberContactIds,
    }), {
      title: "Group member update failed",
      fallbackDetail: "Desktop chat state could not save the updated member list.",
    });

    if (mutation.result) {
      applyDomainSeed(mutation.result, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: payload.sessionId,
      });
      closeTopOverlayPage();
      return;
    }

    if (!mutation.canFallbackLocally) {
      return;
    }

    applyLocalUpdateGroupMembers({
      sessionId: payload.sessionId,
      memberContactIds,
    });
    closeTopOverlayPage();
  }

  async function startLookupChat(query: string) {
    const normalizedQuery = query.trim();
    resetFindPeopleRequestState();
    if (!normalizedQuery) {
      return;
    }

    findPeopleSubmitting.value = true;
    if (isCircleQuery(normalizedQuery)) {
      const joined = await joinCircleFromLookup(
        normalizedQuery,
        shouldReturnToNewMessageAfterCircleJoin() ? { nextOverlay: "new-message" } : {},
      );
      if (!joined) {
        setFindPeopleErrorMessage("Failed to process invite link.");
      }
      findPeopleSubmitting.value = false;
      return;
    }

    if (!activeCircleId.value) {
      findPeopleSubmitting.value = false;
      redirectToCircleSetup();
      return;
    }

    const mutation = await runFallbackEligibleMutation(() => startLookupConversation({
      circleId: activeCircleId.value,
      query: normalizedQuery,
    }), {
      title: "Lookup chat failed",
      fallbackDetail: "Desktop chat state could not create a conversation from this lookup query.",
    });

    if (mutation.result) {
      applyDomainSeed(mutation.result.seed, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: mutation.result.sessionId,
      });
      findPeopleSubmitting.value = false;
      closeAllOverlayPages();
      return;
    }

    if (!mutation.canFallbackLocally) {
      findPeopleSubmitting.value = false;
      setFindPeopleErrorMessage("User not found.");
      return;
    }

    const contact = applyLocalCreateLookupContact(normalizedQuery);
    const sessionId = applyLocalStartConversation(contact.id);
    if (sessionId) {
      findPeopleSubmitting.value = false;
      closeAllOverlayPages();
      return;
    }

    findPeopleSubmitting.value = false;
    setFindPeopleErrorMessage("User not found.");
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
    const mutation = await runFallbackEligibleMutation(() => applyChatSessionAction(payload), {
      title: sessionActionFailureTitle(payload.action),
      fallbackDetail: "Desktop chat state rejected this session action.",
    });
    if (mutation.result) {
      applyDomainSeed(mutation.result, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: payload.action === "unarchive" ? payload.sessionId : selectedSessionId.value,
      });
      return true;
    }

    if (!mutation.canFallbackLocally) {
      return false;
    }

    applyLocalSessionAction(payload);
    return true;
  }

  async function openArchivedSession(sessionId: string) {
    const restored = await handleSessionAction({ sessionId, action: "unarchive" });
    if (restored) {
      closeTopOverlayPage();
    }
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

  function applyLocalUpdateContactRemark(payload: UpdateContactRemarkInput) {
    const nextRemark = payload.remark.trim();
    contacts.value = contacts.value.map((contact) => {
      if (contact.id !== payload.contactId) {
        return contact;
      }

      return {
        ...contact,
        subtitle: nextRemark,
      };
    });
  }

  async function toggleContactBlock(contactId: string) {
    const mutation = await runFallbackEligibleMutation(() => toggleChatContactBlock(contactId), {
      title: "Contact block update failed",
      fallbackDetail: "Desktop chat state could not save the contact block change.",
    });
    if (mutation.result) {
      applyDomainSeed(mutation.result, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: selectedSessionId.value,
      });
      return;
    }

    if (!mutation.canFallbackLocally) {
      return;
    }

    applyLocalToggleContactBlock(contactId);
  }

  async function updateContactRemark(payload: UpdateContactRemarkInput) {
    const mutation = await runFallbackEligibleMutation(() => updateChatContactRemark({
      contactId: payload.contactId,
      remark: payload.remark.trim(),
    }), {
      title: "Save remark failed",
      fallbackDetail: "Desktop chat state could not save the updated contact remark.",
    });
    if (mutation.result) {
      applyDomainSeed(mutation.result, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: selectedSessionId.value,
      });
      return;
    }

    if (!mutation.canFallbackLocally) {
      return;
    }

    applyLocalUpdateContactRemark(payload);
  }

  function toggleGroupMute(sessionId: string) {
    void handleSessionAction({ sessionId, action: "mute" });
  }

  async function leaveGroup(sessionId: string) {
    const removed = await handleSessionAction({ sessionId, action: "delete" });
    if (!removed) {
      return;
    }

    showDetailsDrawer.value = false;
    closeAllOverlayPages();
  }

  function openMemberProfile(contactId: string) {
    openContactProfile(contactId);
  }

  async function sendMessageFromProfile(contactId: string) {
    await startConversation(contactId);
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

  function resolvePublicRelayShortcut(value: string) {
    const normalized = value.trim().toLowerCase();
    if (!normalized) {
      return null;
    }

    switch (normalized) {
      case "0xchat":
        return "wss://relay.0xchat.com";
      case "damus":
        return "wss://relay.damus.io";
      case "nos":
        return "wss://nos.lol";
      case "primal":
        return "wss://relay.primal.net";
      case "yabu":
        return "wss://yabu.me";
      case "nostrband":
        return "wss://relay.nostr.band";
      default:
        return null;
    }
  }

  function normalizeCustomRelayInput(value: string) {
    const trimmed = value.trim();
    if (!trimmed) {
      return "";
    }

    const candidate = resolvePublicRelayShortcut(trimmed) ?? (trimmed.includes("://") ? trimmed : `wss://${trimmed}`);
    try {
      const parsed = new URL(candidate);
      if ((parsed.protocol === "ws:" || parsed.protocol === "wss:") && parsed.hostname) {
        return candidate;
      }
    } catch {
      return trimmed;
    }

    return trimmed;
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
          ? normalizeCustomRelayInput(payload.relay ?? "")
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
    const mutation = await runFallbackEligibleMutation(() => addChatCircle(payload), {
      title: "Add circle failed",
      fallbackDetail: "Desktop shell could not add this circle to native state.",
    });
    if (mutation.result) {
      applyDomainSeed(mutation.result.seed, {
        preferredCircleId: mutation.result.circleId,
        preferredSessionId: selectedSessionId.value,
      });
      forgetRestorableCircleByCircleId(mutation.result.circleId);
      await landOnCircle(mutation.result.circleId);
      return;
    }

    if (!mutation.canFallbackLocally) {
      return;
    }

    const circleId = applyLocalAddCircleFromDirectory(payload);
    if (circleId) {
      forgetRestorableCircleByCircleId(circleId);
      await landOnCircle(circleId);
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
    const mutation = await runFallbackEligibleMutation(() => updateChatCircle(payload), {
      title: "Update circle failed",
      fallbackDetail: "Desktop shell could not save the updated circle settings.",
    });
    if (mutation.result) {
      applyDomainSeed(mutation.result, {
        preferredCircleId: activeCircleId.value,
        preferredSessionId: selectedSessionId.value,
      });
      return;
    }

    if (!mutation.canFallbackLocally) {
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
    const removedCircle = circles.value.find((circle) => circle.id === circleId);
    if (!removedCircle || circles.value.length <= 1) {
      return;
    }

    const removedSessionIds = new Set(
      sessions.value
        .filter((session) => session.circleId === circleId)
        .map((session) => session.id),
    );
    const removedActiveCircle = activeCircleId.value === circleId;
    const mutation = await runFallbackEligibleMutation(() => removeChatCircle(circleId), {
      title: "Remove circle failed",
      fallbackDetail: "Desktop shell could not remove this circle from native state.",
    });

    if (mutation.result) {
      upsertRestorableCircle(buildRestorableCircleEntry(removedCircle));
      applyDomainSeed(mutation.result, {
        preferredCircleId: removedActiveCircle ? mutation.result.circles[0]?.id : activeCircleId.value,
        preferredSessionId: selectedSessionId.value,
      });

      if (removedActiveCircle) {
        await landOnCircle(mutation.result.circles[0]?.id ?? "");
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

    if (!mutation.canFallbackLocally) {
      return;
    }

    upsertRestorableCircle(buildRestorableCircleEntry(removedCircle));
    applyLocalRemoveCircle(circleId);
    if (removedActiveCircle && activeCircleId.value) {
      await landOnCircle(activeCircleId.value);
    }
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
      let result: Awaited<ReturnType<typeof applyTransportCircleAction>>;

      try {
        result = await applyTransportCircleAction({
          circleId,
          action,
          activeCircleId: activeCircleId.value || undefined,
          useTorNetwork: advancedPreferences.value.useTorNetwork,
          experimentalTransport: advancedPreferences.value.experimentalTransport,
        });
      } catch (error) {
        showTransportNotice({
          id: `runtime-action-failed-${circleId}-${action}-${Date.now()}`,
          tone: "warn",
          title: `${circleLabelForRuntimeNotice(circleId)} runtime action failed`,
          detail: describeCommandError(
            error,
            "Desktop transport command did not complete, so the UI kept the previous runtime state.",
          ),
          circleId,
        });
        await refreshTransportSnapshot({ suppressNotice: true });
        return;
      }

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
    const mediaUploadDriver =
      patch.mediaUploadDriver === "auto" ||
      patch.mediaUploadDriver === "local" ||
      patch.mediaUploadDriver === "filedrop" ||
      patch.mediaUploadDriver === "nip96" ||
      patch.mediaUploadDriver === "blossom" ||
      patch.mediaUploadDriver === "minio"
        ? patch.mediaUploadDriver
        : undefined;
    const mediaUploadEndpoint =
      typeof patch.mediaUploadEndpoint === "string"
        ? patch.mediaUploadEndpoint.trim()
        : undefined;
    advancedPreferences.value = {
      ...advancedPreferences.value,
      ...patch,
      ...(mediaUploadDriver ? { mediaUploadDriver } : {}),
      ...(mediaUploadEndpoint !== undefined ? { mediaUploadEndpoint } : {}),
    };
  }

  function openCircleDirectoryFromSettings() {
    replaceTopOverlayPage({ kind: "circle-directory" });
  }

  function pushOverlayPage(page: OverlayPage) {
    if (!overlayPageExists(page)) {
      return;
    }

    applyOverlayPages([...overlayPages.value, page], { mode: "push" });
  }

  function replaceTopOverlayPage(page: OverlayPage) {
    if (!overlayPageExists(page)) {
      return;
    }

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
    findPeopleSubmitting,
    findPeopleErrorMessage,
    isAuthenticated,
    authSession,
    authRuntime,
    authRuntimeBinding,
    userProfile,
    restorableCircles,
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
    replyingToMessage,
    mentionSuggestions,
    showMentionSuggestions,
    mentionSelectionIndex,
    canLoadOlderMessages,
    loadingOlderMessages,
    selectedContact,
    selectedGroup,
    selectedGroupMembers,
    transportSnapshot,
    transportNotice,
    canSendMessages,
    sendBlockedReason,
    runtimeDiagnosticError,
    activeTransportDiagnostic,
    activeOverlayPage,
    activeOverlayContact,
    activeOverlayMessageSession,
    activeOverlayMessage,
    activeOverlayMessageReplyTarget,
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
    openCircleInvitePage,
    openSelfChatConfirmPage,
    openFindPeoplePage,
    openCircleManagement,
    openCircleDetail,
    openDetailsDrawer,
    openContactProfile,
    openMessageDetailPage,
    openGroupSelectMembersPage,
    openGroupCreatePage,
    openProfilePage,
    updateComposerText,
    navigateMentionSuggestions,
    selectMentionSuggestion,
    startReplyToMessage,
    cancelReplyToMessage,
    copyMessageContent,
    copyMessageAttachmentPath,
    openMessageAttachment,
    revealMessageAttachment,
    reportMessage,
    loadOlderMessages,
    sendAttachmentMessage,
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
    updateAuthRuntime,
    syncAuthRuntimeNow,
    handleSessionAction,
    openArchivedSession,
    toggleContactBlock,
    updateContactRemark,
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
    restoreCircleAccess,
    forgetRestorableCircle,
    runTransportCircleAction,
    updateAppPreferences,
    updateNotificationPreferences,
    updateAdvancedPreferences,
    openCircleDirectoryFromSettings,
    closeTopOverlayPage,
    dismissTransportNotice,
  };
}
