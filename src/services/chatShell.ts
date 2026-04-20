import { invoke } from "@tauri-apps/api/core";
import {
  createChatShellSnapshotFromPersistedState,
  createPersistedShellStateFromChatShellSnapshot,
} from "../data/shellDefaults";
import { loadLocalShellState, persistShellStateLocally } from "./shellState";
import type {
  ChatDomainOverview,
  ChatSessionMessageUpdates,
  ChatSessionMessagesPage,
  ChatShellSnapshot,
  LoginCompletionInput,
  LoadSessionMessageUpdatesInput,
  LoadSessionMessagesInput,
  MessageItem,
  PersistedShellState,
  ShellStateSnapshot,
  SessionItem,
  UpdateAuthRuntimeInput,
} from "../types/chat";

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

async function invokeDesktopShellCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T | null> {
  if (!hasTauriRuntime()) {
    return null;
  }

  const result = await invoke<T>(command, args);
  return cloneState(result);
}

export async function loadChatShellSnapshot(
  fallback: PersistedShellState,
): Promise<ChatShellSnapshot> {
  const snapshot = await invokeDesktopShellCommand<ChatShellSnapshot>("load_chat_shell_snapshot");
  if (snapshot) {
    return snapshot;
  }

  return loadChatShellSnapshotLocally(fallback);
}

export function loadChatShellSnapshotLocally(
  fallback: PersistedShellState,
): ChatShellSnapshot {
  const persistedState = loadLocalShellState(fallback);
  return createChatShellSnapshotFromPersistedState(persistedState);
}

export async function syncAuthRuntime(): Promise<ShellStateSnapshot | null> {
  return invokeDesktopShellCommand<ShellStateSnapshot>("sync_auth_runtime");
}

export function persistChatShellSnapshotLocally(snapshot: ChatShellSnapshot) {
  persistShellStateLocally(createPersistedShellStateFromChatShellSnapshot(cloneState(snapshot)));
}

export async function saveChatShellSnapshot(snapshot: ChatShellSnapshot) {
  const nextSnapshot = cloneState(snapshot);
  persistChatShellSnapshotLocally(nextSnapshot);

  if (!hasTauriRuntime()) {
    return;
  }

  await invoke("save_chat_shell_snapshot", {
    snapshot: nextSnapshot,
  });
}

export async function bootstrapAuthSession(
  input: LoginCompletionInput,
): Promise<ShellStateSnapshot | null> {
  return invokeDesktopShellCommand<ShellStateSnapshot>("bootstrap_auth_session", {
    input: cloneState(input),
  });
}

export async function completeLogin(
  input: LoginCompletionInput,
): Promise<ChatShellSnapshot | null> {
  return invokeDesktopShellCommand<ChatShellSnapshot>("complete_login", {
    input: cloneState(input),
  });
}

export async function logoutChatSession(): Promise<ChatShellSnapshot | null> {
  return invokeDesktopShellCommand<ChatShellSnapshot>("logout_chat_session");
}

export async function updateAuthRuntime(
  input: UpdateAuthRuntimeInput,
): Promise<ShellStateSnapshot | null> {
  return invokeDesktopShellCommand<ShellStateSnapshot>("update_auth_runtime", {
    input: cloneState(input),
  });
}

function loadSessionMessagesFallback(
  input: LoadSessionMessagesInput,
  fallbackStore: Record<string, MessageItem[]>,
): ChatSessionMessagesPage {
  const fullMessages = cloneState(fallbackStore[input.sessionId] ?? []);
  const endIndex = input.beforeMessageId
    ? fullMessages.findIndex((message) => message.id === input.beforeMessageId)
    : fullMessages.length;
  const normalizedEndIndex = endIndex >= 0 ? endIndex : fullMessages.length;
  const limit = Math.max(1, Math.min(input.limit ?? 30, 100));
  const startIndex = Math.max(0, normalizedEndIndex - limit);
  const messages = fullMessages.slice(startIndex, normalizedEndIndex);
  const hasMore = startIndex > 0;

  return {
    sessionId: input.sessionId,
    messages,
    hasMore,
    nextBeforeMessageId: hasMore ? messages[0]?.id : undefined,
  };
}

export function loadChatSessionMessagesLocally(
  input: LoadSessionMessagesInput,
  fallbackStore: Record<string, MessageItem[]>,
): ChatSessionMessagesPage {
  return loadSessionMessagesFallback(cloneState(input), fallbackStore);
}

export async function loadChatSessionMessages(
  input: LoadSessionMessagesInput,
  fallbackStore: Record<string, MessageItem[]>,
): Promise<ChatSessionMessagesPage> {
  const nextInput = cloneState(input);
  if (!hasTauriRuntime()) {
    return loadSessionMessagesFallback(nextInput, fallbackStore);
  }

  const page = await invoke<ChatSessionMessagesPage>("load_chat_session_messages", {
    input: nextInput,
  });
  return cloneState(page);
}

function loadSessionMessageUpdatesFallback(
  input: LoadSessionMessageUpdatesInput,
  fallbackStore: Record<string, MessageItem[]>,
): ChatSessionMessageUpdates {
  const fullMessages = cloneState(fallbackStore[input.sessionId] ?? []);
  const startIndex = input.afterMessageId
    ? fullMessages.findIndex((message) => message.id === input.afterMessageId) + 1
    : Math.max(0, fullMessages.length - Math.max(1, Math.min(input.limit ?? 30, 100)));
  const normalizedStartIndex =
    startIndex > 0 ? startIndex : input.afterMessageId ? fullMessages.length : 0;
  const limit = Math.max(1, Math.min(input.limit ?? 30, 100));
  const sliceEnd = normalizedStartIndex + limit;
  const messages = fullMessages.slice(normalizedStartIndex, sliceEnd);
  const hasMore = fullMessages.length > sliceEnd;

  return {
    sessionId: input.sessionId,
    messages,
    hasMore,
    nextAfterMessageId: hasMore ? messages[messages.length - 1]?.id : undefined,
  };
}

export function loadChatSessionMessageUpdatesLocally(
  input: LoadSessionMessageUpdatesInput,
  fallbackStore: Record<string, MessageItem[]>,
): ChatSessionMessageUpdates {
  return loadSessionMessageUpdatesFallback(cloneState(input), fallbackStore);
}

export async function loadChatSessionMessageUpdates(
  input: LoadSessionMessageUpdatesInput,
  fallbackStore: Record<string, MessageItem[]>,
): Promise<ChatSessionMessageUpdates> {
  const nextInput = cloneState(input);
  if (!hasTauriRuntime()) {
    return loadSessionMessageUpdatesFallback(nextInput, fallbackStore);
  }

  const updates = await invoke<ChatSessionMessageUpdates>("load_chat_session_message_updates", {
    input: nextInput,
  });
  return cloneState(updates);
}

export async function loadChatSessionsOverview(
  fallbackSessions: SessionItem[],
): Promise<SessionItem[]> {
  if (!hasTauriRuntime()) {
    return loadChatSessionsOverviewLocally(fallbackSessions);
  }

  const sessions = await invoke<SessionItem[]>("load_chat_sessions_overview");
  return cloneState(sessions);
}

export function loadChatSessionsOverviewLocally(
  fallbackSessions: SessionItem[],
): SessionItem[] {
  return cloneState(fallbackSessions);
}

export async function loadChatDomainOverview(
  fallbackOverview: ChatDomainOverview,
): Promise<ChatDomainOverview> {
  if (!hasTauriRuntime()) {
    return cloneState(fallbackOverview);
  }

  const overview = await invoke<ChatDomainOverview>("load_chat_domain_overview");
  return cloneState(overview);
}

export function loadChatDomainOverviewLocally(
  fallbackOverview: ChatDomainOverview,
): ChatDomainOverview {
  return cloneState(fallbackOverview);
}
