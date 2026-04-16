import { invoke } from "@tauri-apps/api/core";
import type {
  AddCircleInput,
  AddCircleResult,
  ChatDomainSeed,
  SessionActionInput,
  SendMessageInput,
  StartConversationInput,
  StartConversationResult,
  UpdateCircleInput,
} from "../types/chat";

function cloneState<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

export async function sendChatMessage(input: SendMessageInput): Promise<ChatDomainSeed | null> {
  try {
    const seed = await invoke<ChatDomainSeed>("send_chat_message", { input: cloneState(input) });
    return cloneState(seed);
  } catch {
    return null;
  }
}

export async function startDirectConversation(
  input: StartConversationInput,
): Promise<StartConversationResult | null> {
  try {
    const result = await invoke<StartConversationResult>("start_direct_conversation", {
      input: cloneState(input),
    });
    return cloneState(result);
  } catch {
    return null;
  }
}

export async function applyChatSessionAction(
  input: SessionActionInput,
): Promise<ChatDomainSeed | null> {
  try {
    const seed = await invoke<ChatDomainSeed>("apply_chat_session_action", {
      input: cloneState(input),
    });
    return cloneState(seed);
  } catch {
    return null;
  }
}

export async function toggleChatContactBlock(contactId: string): Promise<ChatDomainSeed | null> {
  try {
    const seed = await invoke<ChatDomainSeed>("toggle_chat_contact_block", {
      contactId,
    });
    return cloneState(seed);
  } catch {
    return null;
  }
}

export async function addChatCircle(input: AddCircleInput): Promise<AddCircleResult | null> {
  try {
    const result = await invoke<AddCircleResult>("add_chat_circle", {
      input: cloneState(input),
    });
    return cloneState(result);
  } catch {
    return null;
  }
}

export async function updateChatCircle(input: UpdateCircleInput): Promise<ChatDomainSeed | null> {
  try {
    const seed = await invoke<ChatDomainSeed>("update_chat_circle", {
      input: cloneState(input),
    });
    return cloneState(seed);
  } catch {
    return null;
  }
}

export async function removeChatCircle(circleId: string): Promise<ChatDomainSeed | null> {
  try {
    const seed = await invoke<ChatDomainSeed>("remove_chat_circle", { circleId });
    return cloneState(seed);
  } catch {
    return null;
  }
}
