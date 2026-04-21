import { invoke } from "@tauri-apps/api/core";
import type {
  AddCircleInput,
  AddCircleResult,
  CacheChatMessageMediaInput,
  CachedChatMessageMediaResult,
  ChatDomainSeed,
  CreateGroupConversationInput,
  MergeRemoteDeliveryReceiptsInput,
  MergeRemoteMessagesInput,
  RestoreCircleInput,
  RetryMessageDeliveryInput,
  CleanupChatMediaAssetsResult,
  StoreChatMediaAssetInput,
  StoredChatMediaAsset,
  SendFileMessageInput,
  SendImageMessageInput,
  SendVideoMessageInput,
  SessionActionInput,
  SendMessageInput,
  StartConversationInput,
  StartConversationResult,
  StartLookupConversationInput,
  StartSelfConversationInput,
  UpdateChatMessageMediaRemoteUrlInput,
  UpdateMessageDeliveryStatusInput,
  UpdateContactRemarkInput,
  UpdateGroupMembersInput,
  UpdateGroupNameInput,
  UpdateSessionDraftInput,
  UpdateCircleInput,
  UpdatedChatMessageMediaRemoteUrlResult,
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

async function invokeDesktopMutation<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T | null> {
  if (!hasTauriRuntime()) {
    return null;
  }

  const result = await invoke<T>(command, args);
  return cloneState(result);
}

export async function sendChatMessage(input: SendMessageInput): Promise<ChatDomainSeed | null> {
  return invokeDesktopMutation<ChatDomainSeed>("send_chat_message", {
    input: cloneState(input),
  });
}

export async function sendChatFileMessage(
  input: SendFileMessageInput,
): Promise<ChatDomainSeed | null> {
  return invokeDesktopMutation<ChatDomainSeed>("send_chat_file_message", {
    input: cloneState(input),
  });
}

export async function sendChatImageMessage(
  input: SendImageMessageInput,
): Promise<ChatDomainSeed | null> {
  return invokeDesktopMutation<ChatDomainSeed>("send_chat_image_message", {
    input: cloneState(input),
  });
}

export async function sendChatVideoMessage(
  input: SendVideoMessageInput,
): Promise<ChatDomainSeed | null> {
  return invokeDesktopMutation<ChatDomainSeed>("send_chat_video_message", {
    input: cloneState(input),
  });
}

export async function storeChatMediaAsset(
  input: StoreChatMediaAssetInput,
): Promise<StoredChatMediaAsset | null> {
  return invokeDesktopMutation<StoredChatMediaAsset>("store_chat_media_asset", {
    input: cloneState(input),
  });
}

export async function cleanupChatMediaAssets(): Promise<CleanupChatMediaAssetsResult | null> {
  return invokeDesktopMutation<CleanupChatMediaAssetsResult>("cleanup_chat_media_assets");
}

export async function cacheChatMessageMedia(
  input: CacheChatMessageMediaInput,
): Promise<CachedChatMessageMediaResult | null> {
  return invokeDesktopMutation<CachedChatMessageMediaResult>("cache_chat_message_media", {
    input: cloneState(input),
  });
}

export async function updateChatMessageMediaRemoteUrl(
  input: UpdateChatMessageMediaRemoteUrlInput,
): Promise<UpdatedChatMessageMediaRemoteUrlResult | null> {
  return invokeDesktopMutation<UpdatedChatMessageMediaRemoteUrlResult>(
    "update_chat_message_media_remote_url",
    {
      input: cloneState(input),
    },
  );
}

export async function updateChatSessionDraft(
  input: UpdateSessionDraftInput,
): Promise<ChatDomainSeed | null> {
  return invokeDesktopMutation<ChatDomainSeed>("update_chat_session_draft", {
    input: cloneState(input),
  });
}

export async function updateChatMessageDeliveryStatus(
  input: UpdateMessageDeliveryStatusInput,
): Promise<ChatDomainSeed | null> {
  return invokeDesktopMutation<ChatDomainSeed>("update_chat_message_delivery_status", {
    input: cloneState(input),
  });
}

export async function retryChatMessageDelivery(
  input: RetryMessageDeliveryInput,
): Promise<ChatDomainSeed | null> {
  return invokeDesktopMutation<ChatDomainSeed>("retry_chat_message_delivery", {
    input: cloneState(input),
  });
}

export async function mergeRemoteSessionMessages(
  input: MergeRemoteMessagesInput,
): Promise<ChatDomainSeed | null> {
  return invokeDesktopMutation<ChatDomainSeed>("merge_remote_session_messages", {
    input: cloneState(input),
  });
}

export async function mergeRemoteDeliveryReceipts(
  input: MergeRemoteDeliveryReceiptsInput,
): Promise<ChatDomainSeed | null> {
  return invokeDesktopMutation<ChatDomainSeed>("merge_remote_delivery_receipts", {
    input: cloneState(input),
  });
}

export async function startDirectConversation(
  input: StartConversationInput,
): Promise<StartConversationResult | null> {
  return invokeDesktopMutation<StartConversationResult>("start_direct_conversation", {
    input: cloneState(input),
  });
}

export async function startSelfConversation(
  input: StartSelfConversationInput,
): Promise<StartConversationResult | null> {
  return invokeDesktopMutation<StartConversationResult>("start_self_conversation", {
    input: cloneState(input),
  });
}

export async function createGroupConversation(
  input: CreateGroupConversationInput,
): Promise<StartConversationResult | null> {
  return invokeDesktopMutation<StartConversationResult>("create_group_conversation", {
    input: cloneState(input),
  });
}

export async function startLookupConversation(
  input: StartLookupConversationInput,
): Promise<StartConversationResult | null> {
  return invokeDesktopMutation<StartConversationResult>("start_lookup_conversation", {
    input: cloneState(input),
  });
}

export async function applyChatSessionAction(
  input: SessionActionInput,
): Promise<ChatDomainSeed | null> {
  return invokeDesktopMutation<ChatDomainSeed>("apply_chat_session_action", {
    input: cloneState(input),
  });
}

export async function toggleChatContactBlock(contactId: string): Promise<ChatDomainSeed | null> {
  return invokeDesktopMutation<ChatDomainSeed>("toggle_chat_contact_block", {
    contactId,
  });
}

export async function updateChatContactRemark(
  input: UpdateContactRemarkInput,
): Promise<ChatDomainSeed | null> {
  return invokeDesktopMutation<ChatDomainSeed>("update_chat_contact_remark", {
    input: cloneState(input),
  });
}

export async function updateChatGroupName(
  input: UpdateGroupNameInput,
): Promise<ChatDomainSeed | null> {
  return invokeDesktopMutation<ChatDomainSeed>("update_chat_group_name", {
    input: cloneState(input),
  });
}

export async function updateChatGroupMembers(
  input: UpdateGroupMembersInput,
): Promise<ChatDomainSeed | null> {
  return invokeDesktopMutation<ChatDomainSeed>("update_chat_group_members", {
    input: cloneState(input),
  });
}

export async function addChatCircle(input: AddCircleInput): Promise<AddCircleResult | null> {
  return invokeDesktopMutation<AddCircleResult>("add_chat_circle", {
    input: cloneState(input),
  });
}

export async function restoreChatCircle(
  input: RestoreCircleInput,
): Promise<AddCircleResult | null> {
  return invokeDesktopMutation<AddCircleResult>("restore_chat_circle", {
    input: cloneState(input),
  });
}

export async function updateChatCircle(input: UpdateCircleInput): Promise<ChatDomainSeed | null> {
  return invokeDesktopMutation<ChatDomainSeed>("update_chat_circle", {
    input: cloneState(input),
  });
}

export async function removeChatCircle(circleId: string): Promise<ChatDomainSeed | null> {
  return invokeDesktopMutation<ChatDomainSeed>("remove_chat_circle", { circleId });
}
