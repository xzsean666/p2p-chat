import { invoke } from "@tauri-apps/api/core";
import type { ChatDomainSeed, PersistedShellState } from "../types/chat";

function cloneState<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function toDomainSeed(state: PersistedShellState): ChatDomainSeed {
  return {
    circles: cloneState(state.circles),
    contacts: cloneState(state.contacts),
    sessions: cloneState(state.sessions),
    groups: cloneState(state.groups),
    messageStore: cloneState(state.messageStore),
  };
}

export async function loadChatDomainSeed(fallback: PersistedShellState): Promise<ChatDomainSeed> {
  try {
    const [circles, contacts, sessions, groups, messageStore] = await Promise.all([
      invoke<ChatDomainSeed["circles"]>("load_seed_circles"),
      invoke<ChatDomainSeed["contacts"]>("load_seed_contacts"),
      invoke<ChatDomainSeed["sessions"]>("load_seed_sessions"),
      invoke<ChatDomainSeed["groups"]>("load_seed_groups"),
      invoke<ChatDomainSeed["messageStore"]>("load_seed_message_store"),
    ]);

    return {
      circles: cloneState(circles),
      contacts: cloneState(contacts),
      sessions: cloneState(sessions),
      groups: cloneState(groups),
      messageStore: cloneState(messageStore),
    };
  } catch {
    return toDomainSeed(fallback);
  }
}

export async function saveChatDomainSeed(seed: ChatDomainSeed) {
  try {
    await invoke("save_chat_domain_seed", { seed: cloneState(seed) });
  } catch {
    // Browser mode falls back to shell snapshot persistence only.
  }
}
