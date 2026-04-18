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
  LoadSessionMessageUpdatesInput,
  LoadSessionMessagesInput,
  MessageItem,
  PersistedShellState,
  SessionItem,
} from "../types/chat";

function cloneState<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

export async function loadChatShellSnapshot(
  fallback: PersistedShellState,
): Promise<ChatShellSnapshot> {
  try {
    const snapshot = await invoke<ChatShellSnapshot>("load_chat_shell_snapshot");
    return cloneState(snapshot);
  } catch {
    const persistedState = loadLocalShellState(fallback);
    return createChatShellSnapshotFromPersistedState(persistedState);
  }
}

export function persistChatShellSnapshotLocally(snapshot: ChatShellSnapshot) {
  persistShellStateLocally(createPersistedShellStateFromChatShellSnapshot(cloneState(snapshot)));
}

export async function saveChatShellSnapshot(snapshot: ChatShellSnapshot) {
  const nextSnapshot = cloneState(snapshot);
  persistChatShellSnapshotLocally(nextSnapshot);

  try {
    await invoke("save_chat_shell_snapshot", {
      snapshot: nextSnapshot,
    });
  } catch {
    // Browser mode keeps the local shell snapshot as the durable fallback.
  }
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

export async function loadChatSessionMessages(
  input: LoadSessionMessagesInput,
  fallbackStore: Record<string, MessageItem[]>,
): Promise<ChatSessionMessagesPage> {
  const nextInput = cloneState(input);

  try {
    const page = await invoke<ChatSessionMessagesPage>("load_chat_session_messages", {
      input: nextInput,
    });
    return cloneState(page);
  } catch {
    return loadSessionMessagesFallback(nextInput, fallbackStore);
  }
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

export async function loadChatSessionMessageUpdates(
  input: LoadSessionMessageUpdatesInput,
  fallbackStore: Record<string, MessageItem[]>,
): Promise<ChatSessionMessageUpdates> {
  const nextInput = cloneState(input);

  try {
    const updates = await invoke<ChatSessionMessageUpdates>("load_chat_session_message_updates", {
      input: nextInput,
    });
    return cloneState(updates);
  } catch {
    return loadSessionMessageUpdatesFallback(nextInput, fallbackStore);
  }
}

export async function loadChatSessionsOverview(
  fallbackSessions: SessionItem[],
): Promise<SessionItem[]> {
  try {
    const sessions = await invoke<SessionItem[]>("load_chat_sessions_overview");
    return cloneState(sessions);
  } catch {
    return cloneState(fallbackSessions);
  }
}

export async function loadChatDomainOverview(
  fallbackOverview: ChatDomainOverview,
): Promise<ChatDomainOverview> {
  try {
    const overview = await invoke<ChatDomainOverview>("load_chat_domain_overview");
    return cloneState(overview);
  } catch {
    return cloneState(fallbackOverview);
  }
}
