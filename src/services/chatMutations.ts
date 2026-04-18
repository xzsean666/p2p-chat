import { invoke } from "@tauri-apps/api/core";
import type {
  AddCircleInput,
  AddCircleResult,
  ChatDomainSeed,
  CreateGroupConversationInput,
  MergeRemoteDeliveryReceiptsInput,
  MergeRemoteMessagesInput,
  RetryMessageDeliveryInput,
  SessionActionInput,
  SendMessageInput,
  StartConversationInput,
  StartConversationResult,
  StartLookupConversationInput,
  StartSelfConversationInput,
  UpdateMessageDeliveryStatusInput,
  UpdateGroupMembersInput,
  UpdateGroupNameInput,
  UpdateSessionDraftInput,
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

export async function updateChatSessionDraft(
  input: UpdateSessionDraftInput,
): Promise<ChatDomainSeed | null> {
  try {
    const seed = await invoke<ChatDomainSeed>("update_chat_session_draft", {
      input: cloneState(input),
    });
    return cloneState(seed);
  } catch {
    return null;
  }
}

export async function updateChatMessageDeliveryStatus(
  input: UpdateMessageDeliveryStatusInput,
): Promise<ChatDomainSeed | null> {
  try {
    const seed = await invoke<ChatDomainSeed>("update_chat_message_delivery_status", {
      input: cloneState(input),
    });
    return cloneState(seed);
  } catch {
    return null;
  }
}

export async function retryChatMessageDelivery(
  input: RetryMessageDeliveryInput,
): Promise<ChatDomainSeed | null> {
  try {
    const seed = await invoke<ChatDomainSeed>("retry_chat_message_delivery", {
      input: cloneState(input),
    });
    return cloneState(seed);
  } catch {
    return null;
  }
}

export async function mergeRemoteSessionMessages(
  input: MergeRemoteMessagesInput,
): Promise<ChatDomainSeed | null> {
  try {
    const seed = await invoke<ChatDomainSeed>("merge_remote_session_messages", {
      input: cloneState(input),
    });
    return cloneState(seed);
  } catch {
    return null;
  }
}

export async function mergeRemoteDeliveryReceipts(
  input: MergeRemoteDeliveryReceiptsInput,
): Promise<ChatDomainSeed | null> {
  try {
    const seed = await invoke<ChatDomainSeed>("merge_remote_delivery_receipts", {
      input: cloneState(input),
    });
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

export async function startSelfConversation(
  input: StartSelfConversationInput,
): Promise<StartConversationResult | null> {
  try {
    const result = await invoke<StartConversationResult>("start_self_conversation", {
      input: cloneState(input),
    });
    return cloneState(result);
  } catch {
    return null;
  }
}

export async function createGroupConversation(
  input: CreateGroupConversationInput,
): Promise<StartConversationResult | null> {
  try {
    const result = await invoke<StartConversationResult>("create_group_conversation", {
      input: cloneState(input),
    });
    return cloneState(result);
  } catch {
    return null;
  }
}

export async function startLookupConversation(
  input: StartLookupConversationInput,
): Promise<StartConversationResult | null> {
  try {
    const result = await invoke<StartConversationResult>("start_lookup_conversation", {
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

export async function updateChatGroupName(
  input: UpdateGroupNameInput,
): Promise<ChatDomainSeed | null> {
  try {
    const seed = await invoke<ChatDomainSeed>("update_chat_group_name", {
      input: cloneState(input),
    });
    return cloneState(seed);
  } catch {
    return null;
  }
}

export async function updateChatGroupMembers(
  input: UpdateGroupMembersInput,
): Promise<ChatDomainSeed | null> {
  try {
    const seed = await invoke<ChatDomainSeed>("update_chat_group_members", {
      input: cloneState(input),
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
