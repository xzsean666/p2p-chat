<script setup lang="ts">
import { computed, ref, watch } from "vue";
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import Menu from "primevue/menu";
import ScrollPanel from "primevue/scrollpanel";
import Textarea from "primevue/textarea";
import type { MenuItem } from "primevue/menuitem";
import SelfChatIcon from "./SelfChatIcon.vue";
import {
  fileMessageLocalPath,
  fileMessageMetaLabel,
  fileMessageRemoteUrl,
} from "../features/chat/fileMessageMeta";
import {
  imageMessageLocalPath,
  imageMessageMetaLabel,
  imageMessagePreviewUrl,
  imageMessageRemoteUrl,
} from "../features/chat/imageMessageMeta";
import {
  videoMessageLocalPath,
  videoMessageMetaLabel,
  videoMessagePreviewUrl,
  videoMessageRemoteUrl,
} from "../features/chat/videoMessageMeta";
import {
  resolveMessageAuthorInitials,
  resolveMessageAuthorLabel,
} from "../features/chat/messageAuthor";
import type { CircleItem, ContactItem, MessageItem, SessionItem } from "../types/chat";

const props = defineProps<{
  session: SessionItem | null;
  activeCircle: CircleItem | null;
  messages: MessageItem[];
  replyingToMessage: MessageItem | null;
  mentionSuggestions: ContactItem[];
  showMentionSuggestions: boolean;
  mentionSelectionIndex: number;
  canLoadOlderMessages: boolean;
  loadingOlderMessages: boolean;
  composerText: string;
  showMessageInfo: boolean;
  canSendMessages?: boolean;
  sendBlockedReason?: string;
  runtimeError?: string;
  presentation?: "panel" | "page";
  showBackButton?: boolean;
}>();

const emit = defineEmits<{
  (event: "back"): void;
  (event: "load-older"): void;
  (event: "update:composerText", value: string): void;
  (event: "mention-navigate", direction: 1 | -1): void;
  (event: "mention-select", contactId?: string): void;
  (event: "reply-message", messageId: string): void;
  (event: "cancel-reply"): void;
  (event: "open-message-detail", messageId: string): void;
  (event: "attach-file", file: File): void;
  (event: "copy-message", messageId: string): void;
  (event: "copy-attachment-path", messageId: string): void;
  (event: "open-attachment", messageId: string): void;
  (event: "report-message", payload: { messageId: string; reason: string }): void;
  (event: "reveal-attachment", messageId: string): void;
  (event: "send"): void;
  (event: "retry-message", messageId: string): void;
  (event: "open-profile"): void;
  (event: "open-details"): void;
}>();

const attachmentInput = ref<HTMLInputElement | null>(null);
const messageActionMenu = ref<{
  show: (event: Event) => void;
  hide: () => void;
} | null>(null);
const activeActionMessageId = ref<string | null>(null);
const reportDialogVisible = ref(false);
const reportTargetMessageId = ref<string | null>(null);
const reportReason = ref("");
const reportReasonOptions = [
  "Spam",
  "Violence",
  "Child abuse",
  "Pornography",
  "Copyright",
  "Illegal drugs",
  "Personal details",
] as const;

const subtitle = computed(() => {
  if (!props.session) {
    return "";
  }

  if (props.session.kind === "self") {
    return "Add notes to yourself here.";
  }

  if (props.session.kind === "group") {
    return `${props.session.members ?? 0} members · encrypted group`;
  }

  return "Direct message · end-to-end relay";
});

const normalizedSendBlockedReason = computed(() => {
  return props.sendBlockedReason?.trim() ?? "";
});

const runtimeDiagnosticMessage = computed(() => {
  const runtimeError = props.runtimeError?.trim() ?? "";
  if (!runtimeError || runtimeError === normalizedSendBlockedReason.value) {
    return "";
  }

  return `Latest runtime error: ${runtimeError}`;
});

const emptyStateTitle = computed(() => {
  return props.activeCircle ? "Select a conversation" : "Add or restore a circle";
});

const emptyStateDescription = computed(() => {
  return props.activeCircle
    ? "Choose a session on the left to open the chat page."
    : "Join or restore a circle first, then pick a conversation to start chatting.";
});

const activeActionMessage = computed(() => {
  if (!activeActionMessageId.value) {
    return null;
  }

  return props.messages.find((message) => message.id === activeActionMessageId.value) ?? null;
});

const reportTargetMessage = computed(() => {
  if (!reportTargetMessageId.value) {
    return null;
  }

  return props.messages.find((message) => message.id === reportTargetMessageId.value) ?? null;
});

const messageActionItems = computed<MenuItem[]>(() => {
  const message = activeActionMessage.value;
  if (!message || message.kind === "system") {
    return [];
  }

  const items: MenuItem[] = [
    {
      label: "Reply",
      icon: "pi pi-reply",
      command: () => emit("reply-message", message.id),
    },
    {
      label: copyActionLabel(message),
      icon: "pi pi-copy",
      command: () => emit("copy-message", message.id),
    },
  ];

  if (props.showMessageInfo) {
    items.push({
      label: "Message detail",
      icon: "pi pi-info-circle",
      command: () => emit("open-message-detail", message.id),
    });
  }

  if (messageHasLocalAttachmentPath(message)) {
    items.push(
      {
        label: "Open attachment",
        icon: "pi pi-folder-open",
        command: () => emit("open-attachment", message.id),
      },
      {
        label: "Reveal in folder",
        icon: "pi pi-external-link",
        command: () => emit("reveal-attachment", message.id),
      },
      {
        label: "Copy local path",
        icon: "pi pi-link",
        command: () => emit("copy-attachment-path", message.id),
      },
    );
  }

  if (message.author === "peer") {
    items.push(
      { separator: true },
      {
        label: "Report message",
        icon: "pi pi-flag",
        command: () => openReportDialog(message),
      },
    );
  }

  return items;
});

function messageDeliveryLabel(message: MessageItem) {
  switch (message.deliveryStatus) {
    case "sending":
      return "Sending";
    case "failed":
      return "Failed";
    case "sent":
      return message.ackedAt ? "Delivered" : "Sent";
    default:
      return "";
  }
}

function messagePreviewSnippet(message: Pick<MessageItem, "kind" | "body" | "meta">) {
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

function messageTextSegments(body: string) {
  return body.split(/(\s+)/).flatMap((segment, index) => {
    if (!segment) {
      return [];
    }

    const mentionMatch = segment.match(/^(@[A-Za-z0-9_.-]+)([.,!?;:]*)$/);
    if (!mentionMatch) {
      return [{ key: `${index}-plain`, text: segment, mention: false }];
    }

    const [, mention, trailing] = mentionMatch;
    return [
      { key: `${index}-mention`, text: mention, mention: true },
      ...(trailing ? [{ key: `${index}-trailing`, text: trailing, mention: false }] : []),
    ];
  });
}

function composerReplyAuthorLabel(message: MessageItem) {
  return resolveMessageAuthorLabel(props.session, message);
}

function messageAuthorLabel(message: MessageItem) {
  return resolveMessageAuthorLabel(props.session, message);
}

function messageAuthorInitials(message: MessageItem) {
  return resolveMessageAuthorInitials(props.session, message);
}

function isPeerClusterBoundary(message: MessageItem, messageIndex: number) {
  if (message.author !== "peer") {
    return false;
  }

  for (let index = messageIndex - 1; index >= 0; index -= 1) {
    const previousMessage = props.messages[index];
    if (previousMessage.kind === "system") {
      continue;
    }

    return (
      previousMessage.author !== "peer" ||
      previousMessage.authorContactId !== message.authorContactId ||
      previousMessage.authorName !== message.authorName
    );
  }

  return true;
}

function showMessageAuthor(message: MessageItem, messageIndex: number) {
  return props.session?.kind === "group" && isPeerClusterBoundary(message, messageIndex);
}

function showPeerAvatar(message: MessageItem, messageIndex: number) {
  if (message.author !== "peer" || props.session?.kind === "self") {
    return false;
  }

  if (props.session?.kind === "group") {
    return isPeerClusterBoundary(message, messageIndex);
  }

  return true;
}

function showPeerAvatarSpacer(message: MessageItem, messageIndex: number) {
  return props.session?.kind === "group" && message.author === "peer" && !isPeerClusterBoundary(message, messageIndex);
}

function mentionSuggestionCaption(contact: ContactItem) {
  return contact.subtitle || contact.name;
}

function copyActionLabel(message: MessageItem) {
  switch (message.kind) {
    case "image":
      return "Copy image name";
    case "video":
      return "Copy video name";
    case "file":
      return "Copy file name";
    case "audio":
      return "Copy audio label";
    default:
      return "Copy message";
  }
}

function messageHasLocalAttachmentPath(message: MessageItem) {
  switch (message.kind) {
    case "file":
      return !!fileMessageLocalPath(message) || !!fileMessageRemoteUrl(message);
    case "image":
      return !!imageMessageLocalPath(message) || !!imageMessageRemoteUrl(message);
    case "video":
      return !!videoMessageLocalPath(message) || !!videoMessageRemoteUrl(message);
    default:
      return false;
  }
}

function closeMessageActionMenu() {
  messageActionMenu.value?.hide();
  activeActionMessageId.value = null;
}

function openMessageActionMenu(event: Event, message: MessageItem) {
  activeActionMessageId.value = message.id;
  messageActionMenu.value?.show(event);
}

function openReportDialog(message: MessageItem) {
  if (message.author !== "peer") {
    return;
  }

  reportTargetMessageId.value = message.id;
  reportReason.value = reportReasonOptions[0];
  reportDialogVisible.value = true;
}

function closeReportDialog() {
  reportDialogVisible.value = false;
  reportTargetMessageId.value = null;
  reportReason.value = "";
}

function submitMessageReport() {
  if (!reportTargetMessage.value || !reportReason.value) {
    return;
  }

  emit("report-message", {
    messageId: reportTargetMessage.value.id,
    reason: reportReason.value,
  });
  closeReportDialog();
}

function triggerAttachmentPicker() {
  if (!props.canSendMessages) {
    return;
  }

  attachmentInput.value?.click();
}

function handleAttachmentChange(event: Event) {
  const input = event.target as HTMLInputElement | null;
  const file = input?.files?.[0];
  if (!file) {
    return;
  }

  emit("attach-file", file);
  input.value = "";
}

function chatPaneClasses() {
  return ["chat-pane", props.presentation === "page" ? "page" : "panel"];
}

watch(
  () => props.session?.id,
  () => {
    closeMessageActionMenu();
    closeReportDialog();
  },
);
</script>

<template>
  <section :class="chatPaneClasses()">
    <template v-if="session">
      <Menu
        ref="messageActionMenu"
        :model="messageActionItems"
        popup
        @hide="activeActionMessageId = null"
      />

      <header class="chat-header">
        <Button
          v-if="showBackButton"
          icon="pi pi-arrow-left"
          rounded
          text
          severity="secondary"
          aria-label="Back"
          class="chat-back-button"
          @click="emit('back')"
        />

        <button type="button" class="chat-title" @click="emit('open-profile')">
          <SelfChatIcon v-if="session.kind === 'self'" size="lg" />
          <Avatar
            v-else
            :label="session.initials"
            shape="circle"
            class="contact-avatar"
          />

          <div>
            <h2>{{ session.name }}</h2>
            <p>{{ subtitle }}</p>
          </div>
        </button>

        <div class="chat-actions">
          <Button
            icon="pi pi-info-circle"
            rounded
            text
            severity="secondary"
            @click="emit('open-details')"
          />
        </div>
      </header>

      <ScrollPanel class="message-scroll">
        <div class="message-list">
          <div v-if="canLoadOlderMessages" class="message-history-action">
            <button
              type="button"
              class="history-link"
              :disabled="loadingOlderMessages"
              @click="emit('load-older')"
            >
              {{ loadingOlderMessages ? "Loading earlier messages..." : "Earlier messages" }}
            </button>
          </div>

          <template v-for="(message, messageIndex) in messages" :key="message.id">
            <div v-if="message.kind === 'system'" class="system-line">
              {{ message.body }}
            </div>

            <div
              v-else
              :class="[
                'message-row',
                {
                  mine: message.author === 'me',
                  peer: message.author === 'peer',
                },
              ]"
            >
              <Avatar
                v-if="showPeerAvatar(message, messageIndex)"
                :label="messageAuthorInitials(message)"
                shape="circle"
                class="bubble-avatar"
              />
              <span
                v-else-if="showPeerAvatarSpacer(message, messageIndex)"
                class="bubble-avatar-spacer"
                aria-hidden="true"
              ></span>

              <div class="message-cluster">
                <span v-if="showMessageAuthor(message, messageIndex)" class="message-author">
                  {{ messageAuthorLabel(message) }}
                </span>
                <div
                  :class="[
                    'message-bubble',
                    `kind-${message.kind}`,
                    {
                      mine: message.author === 'me',
                    },
                  ]"
                  @contextmenu.prevent="openMessageActionMenu($event, message)"
                >
                  <div v-if="message.replyTo" class="reply-preview">
                    <span class="reply-author">{{ message.replyTo.authorLabel }}</span>
                    <p>{{ message.replyTo.snippet }}</p>
                  </div>

                  <template v-if="message.kind === 'text'">
                    <p class="message-text">
                      <template
                        v-for="segment in messageTextSegments(message.body)"
                        :key="`${message.id}-segment-${segment.key}`"
                      >
                        <span
                          :class="[
                            'message-text-segment',
                            { 'message-text-mention': segment.mention },
                          ]"
                        >
                          {{ segment.text }}
                        </span>
                      </template>
                    </p>
                  </template>

                  <template v-else-if="message.kind === 'file'">
                    <div class="file-card">
                      <i class="pi pi-file"></i>
                      <div>
                        <strong>{{ message.body }}</strong>
                        <span>{{ fileMessageMetaLabel(message) || message.meta }}</span>
                      </div>
                    </div>
                  </template>

                  <template v-else-if="message.kind === 'image'">
                    <div class="image-card">
                      <img
                        v-if="imageMessagePreviewUrl(message)"
                        :src="imageMessagePreviewUrl(message)"
                        :alt="message.body"
                        class="image-preview"
                      />
                      <div class="media-copy">
                        <strong>{{ message.body }}</strong>
                        <span>{{ imageMessageMetaLabel(message) || message.meta }}</span>
                      </div>
                    </div>
                  </template>

                  <template v-else-if="message.kind === 'video'">
                    <div class="video-card">
                      <video
                        v-if="videoMessagePreviewUrl(message)"
                        :src="videoMessagePreviewUrl(message)"
                        class="video-preview"
                        controls
                        playsinline
                        preload="metadata"
                      />
                      <div class="media-copy">
                        <strong>{{ message.body }}</strong>
                        <span>{{ videoMessageMetaLabel(message) || message.meta }}</span>
                      </div>
                    </div>
                  </template>

                  <template v-else-if="message.kind === 'audio'">
                    <div class="audio-card">
                      <i class="pi pi-play-circle"></i>
                      <div class="audio-wave">
                        <span v-for="wave in 18" :key="wave"></span>
                      </div>
                      <strong>{{ message.meta }}</strong>
                    </div>
                  </template>
                </div>
                <div class="message-meta">
                  <span class="message-time">{{ message.time }}</span>
                  <Button
                    icon="pi pi-ellipsis-h"
                    text
                    rounded
                    severity="secondary"
                    aria-label="Open message actions"
                    class="message-action-menu-trigger"
                    @click="openMessageActionMenu($event, message)"
                  />
                  <span
                    v-if="message.author === 'me' && message.deliveryStatus"
                    :class="['message-status', `status-${message.deliveryStatus}`]"
                  >
                    {{ messageDeliveryLabel(message) }}
                  </span>
                  <Button
                    v-if="message.author === 'me' && message.deliveryStatus === 'failed'"
                    label="Retry"
                    size="small"
                    text
                    severity="danger"
                    class="message-retry"
                    :disabled="!canSendMessages"
                    @click="emit('retry-message', message.id)"
                  />
                </div>
              </div>
            </div>
          </template>
        </div>
      </ScrollPanel>

      <div class="composer">
        <div class="composer-actions">
          <input
            ref="attachmentInput"
            type="file"
            class="composer-file-input"
            @change="handleAttachmentChange"
          />
          <Button
            icon="pi pi-paperclip"
            rounded
            text
            severity="contrast"
            aria-label="Attach file"
            :disabled="!canSendMessages"
            @click="triggerAttachmentPicker"
          />
        </div>

        <div v-if="replyingToMessage" class="composer-reply">
          <div class="composer-reply-copy">
            <span class="reply-author">{{ composerReplyAuthorLabel(replyingToMessage) }}</span>
            <p>{{ messagePreviewSnippet(replyingToMessage) }}</p>
          </div>
          <Button
            icon="pi pi-times"
            rounded
            text
            severity="secondary"
            aria-label="Cancel reply"
            @click="emit('cancel-reply')"
          />
        </div>

        <div
          v-if="!canSendMessages && normalizedSendBlockedReason"
          class="composer-state composer-state-blocked"
        >
          {{ normalizedSendBlockedReason }}
        </div>

        <div
          v-if="runtimeDiagnosticMessage"
          class="composer-state composer-state-warning"
        >
          {{ runtimeDiagnosticMessage }}
        </div>

        <div v-if="showMentionSuggestions" class="mention-suggestions">
          <button
            v-for="(contact, suggestionIndex) in mentionSuggestions"
            :key="contact.id"
            type="button"
            :class="[
              'mention-suggestion',
              { selected: suggestionIndex === mentionSelectionIndex },
            ]"
            @click="emit('mention-select', contact.id)"
          >
            <span class="mention-handle">{{ contact.handle }}</span>
            <span class="mention-caption">{{ mentionSuggestionCaption(contact) }}</span>
          </button>
        </div>

        <Textarea
          :model-value="composerText"
          auto-resize
          rows="1"
          class="composer-input"
          :placeholder="canSendMessages ? 'Write a message' : 'Sending is unavailable for this account state'"
          @update:model-value="emit('update:composerText', String($event))"
          @keydown.down.prevent="showMentionSuggestions && emit('mention-navigate', 1)"
          @keydown.up.prevent="showMentionSuggestions && emit('mention-navigate', -1)"
          @keydown.tab.prevent="showMentionSuggestions && emit('mention-select')"
          @keydown.enter.exact.prevent="showMentionSuggestions ? emit('mention-select') : canSendMessages && emit('send')"
        />

        <Button
          icon="pi pi-send"
          rounded
          severity="contrast"
          :disabled="!canSendMessages"
          @click="emit('send')"
        />
      </div>

      <Dialog
        v-model:visible="reportDialogVisible"
        modal
        dismissable-mask
        header="Report message"
        class="message-report-dialog"
        @hide="closeReportDialog"
      >
        <div class="report-dialog-copy">
          <p>
            Choose a reason for this peer message. The desktop rebuild will copy a moderation
            handoff package to your clipboard until remote report submission is wired.
          </p>
          <div v-if="reportTargetMessage" class="report-target-preview">
            <span class="reply-author">Message</span>
            <p>{{ messagePreviewSnippet(reportTargetMessage) }}</p>
          </div>
        </div>

        <div class="report-reason-list">
          <button
            v-for="reason in reportReasonOptions"
            :key="reason"
            type="button"
            :class="['report-reason-option', { selected: reportReason === reason }]"
            @click="reportReason = reason"
          >
            {{ reason }}
          </button>
        </div>

        <template #footer>
          <div class="report-dialog-actions">
            <Button label="Cancel" text severity="secondary" @click="closeReportDialog" />
            <Button
              label="Copy report package"
              severity="contrast"
              :disabled="!reportTargetMessage || !reportReason"
              @click="submitMessageReport"
            />
          </div>
        </template>
      </Dialog>
    </template>

    <div v-else class="chat-empty">
      <div class="empty-mark">
        <i class="pi pi-comment"></i>
      </div>
      <h2>{{ emptyStateTitle }}</h2>
      <p>{{ emptyStateDescription }}</p>
    </div>
  </section>
</template>

<style scoped>
.chat-pane {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr) auto;
  gap: 0;
  min-height: 0;
  padding: 10px 0 0;
  border-radius: 0;
  background: transparent;
  border: 0;
  box-shadow: none;
}

.chat-pane.page {
  min-height: 100vh;
  padding:
    max(12px, env(safe-area-inset-top))
    16px
    max(18px, env(safe-area-inset-bottom));
  border: 0;
  border-radius: 0;
  background: color-mix(in srgb, var(--shell-surface-strong) 96%, white);
  box-shadow: none;
}

.chat-header,
.chat-title,
.chat-actions,
.composer,
.composer-actions,
.message-history-action,
.message-row,
.file-card,
.audio-card,
.message-meta {
  display: flex;
}

.composer-state {
  flex: 1 1 100%;
  padding: 0 4px;
  color: var(--shell-text-muted);
  font-size: 0.74rem;
  line-height: 1.45;
}

.composer-file-input {
  display: none;
}

.message-report-dialog :deep(.p-dialog-content) {
  display: grid;
  gap: 16px;
}

.report-dialog-copy,
.report-reason-list,
.report-dialog-actions {
  display: grid;
}

.report-dialog-copy {
  gap: 12px;
}

.report-dialog-copy p {
  margin: 0;
  line-height: 1.55;
  color: var(--shell-text-muted);
}

.report-target-preview {
  display: grid;
  gap: 4px;
  padding: 12px 14px;
  border-radius: 16px;
  background: var(--shell-surface-soft);
  border: 1px solid var(--shell-border-soft);
}

.report-target-preview p {
  color: var(--shell-text-default);
}

.report-reason-list {
  gap: 10px;
}

.report-reason-option {
  width: 100%;
  padding: 12px 14px;
  border-radius: 16px;
  border: 1px solid var(--shell-border);
  background: var(--shell-surface);
  color: var(--shell-text-default);
  text-align: left;
  font: inherit;
  cursor: pointer;
  transition:
    border-color 0.2s ease,
    background-color 0.2s ease,
    transform 0.2s ease;
}

.report-reason-option:hover {
  background: var(--shell-surface-soft);
}

.report-reason-option.selected {
  border-color: var(--shell-selected-border);
  background: color-mix(in srgb, var(--shell-selected) 72%, white);
  transform: translateY(-1px);
}

.report-dialog-actions {
  width: 100%;
  grid-auto-flow: column;
  justify-content: end;
  gap: 10px;
}

.composer-state-blocked {
  color: #a26918;
}

.composer-state-warning {
  color: #b15e53;
}

.mention-suggestions {
  display: grid;
  width: 100%;
  gap: 4px;
  padding: 6px;
  border-radius: 14px;
  background: color-mix(in srgb, var(--shell-surface-soft) 82%, white);
  border: 0;
}

.mention-suggestion {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  width: 100%;
  padding: 9px 10px;
  border: 0;
  border-radius: 10px;
  background: transparent;
  text-align: left;
  cursor: pointer;
}

.mention-suggestion.selected {
  background: color-mix(in srgb, var(--shell-selected) 48%, transparent);
}

.mention-handle {
  color: #1d4f91;
  font-weight: 700;
}

.mention-caption {
  color: var(--shell-text-muted);
  font-size: 0.9rem;
}

.chat-header {
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 0 16px 12px;
  border-bottom: 1px solid color-mix(in srgb, var(--shell-border-soft) 82%, transparent);
}

.chat-back-button {
  flex: 0 0 auto;
}

.chat-title {
  align-items: center;
  gap: 12px;
  min-width: 0;
  padding: 0;
  border: 0;
  background: transparent;
  text-align: left;
  cursor: pointer;
}

.chat-title h2,
.chat-title p {
  margin: 0;
}

.chat-title p {
  margin-top: 2px;
  color: var(--shell-text-muted);
  font-size: 0.79rem;
}

.chat-actions {
  gap: 8px;
}

.contact-avatar,
.bubble-avatar {
  background: var(--shell-avatar-bg);
  color: var(--shell-avatar-text);
  font-weight: 700;
}

.contact-avatar {
  width: 40px;
  height: 40px;
}

.bubble-avatar {
  width: 32px;
  height: 32px;
  margin-bottom: 16px;
}

.bubble-avatar-spacer {
  flex: 0 0 32px;
  width: 32px;
}

.message-scroll {
  min-height: 0;
}

.message-list {
  padding: 10px 16px 0;
}

.message-history-action {
  justify-content: center;
  margin-bottom: 8px;
}

.history-link {
  padding: 4px 0;
  border: 0;
  background: transparent;
  color: var(--shell-text-soft);
  font: inherit;
  font-size: 0.76rem;
  cursor: pointer;
}

.history-link:disabled {
  opacity: 0.56;
  cursor: default;
}

.system-line {
  margin: 10px auto 16px;
  width: fit-content;
  padding: 6px 10px;
  border-radius: 999px;
  background: color-mix(in srgb, var(--shell-surface-soft) 72%, transparent);
  color: var(--shell-text-soft);
  font-size: 0.76rem;
}

.message-row {
  align-items: flex-end;
  gap: 10px;
  margin-bottom: 10px;
}

.message-row.mine {
  justify-content: flex-end;
}

.message-cluster {
  display: grid;
  gap: 3px;
  max-width: min(80%, 620px);
}

.message-row.mine .message-cluster {
  justify-items: end;
}

.message-author {
  padding: 0 2px;
  color: var(--shell-text-muted);
  font-size: 0.74rem;
  font-weight: 600;
  letter-spacing: 0.01em;
}

.message-bubble {
  border-radius: 18px;
  padding: 11px 13px;
  background: color-mix(in srgb, var(--shell-surface-soft) 88%, white);
  border: 0;
  box-shadow: none;
}

.message-bubble.mine {
  background: color-mix(in srgb, var(--shell-selected) 76%, white);
}

.message-bubble p {
  margin: 0;
  line-height: 1.58;
}

.message-text {
  white-space: pre-wrap;
  word-break: break-word;
}

.message-text-mention {
  color: #1d4f91;
  font-weight: 700;
}

.reply-preview,
.composer-reply {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 9px 11px;
  border-radius: 14px;
  background: color-mix(in srgb, var(--shell-surface-soft) 78%, white);
  border: 0;
}

.reply-preview {
  margin-bottom: 10px;
  display: grid;
  gap: 4px;
}

.reply-author {
  color: var(--shell-text-soft);
  font-size: 0.72rem;
  font-weight: 700;
  letter-spacing: 0.04em;
}

.reply-preview p,
.composer-reply p {
  margin: 0;
  color: var(--shell-text-default);
  line-height: 1.45;
}

.message-time {
  color: var(--shell-text-soft);
  font-size: 0.74rem;
}

.message-meta {
  align-items: center;
  gap: 5px;
  min-height: 1.5rem;
}

.message-action-menu-trigger {
  width: 1.6rem;
  height: 1.6rem;
  opacity: 0;
  pointer-events: none;
  transition: opacity 0.16s ease;
}

.message-row:hover .message-action-menu-trigger,
.message-row:focus-within .message-action-menu-trigger {
  opacity: 1;
  pointer-events: auto;
}

.message-status {
  font-size: 0.72rem;
  font-weight: 600;
}

.message-status.status-sending {
  color: #7a8ca3;
}

.message-status.status-sent {
  color: #5b7e69;
}

.message-status.status-failed {
  color: #c95a48;
}

.message-retry {
  padding: 0;
  font-size: 0.74rem;
}

.file-card,
.image-card,
.video-card,
.audio-card {
  align-items: center;
  gap: 10px;
}

.file-card i,
.audio-card i {
  font-size: 1.2rem;
  color: color-mix(in srgb, var(--shell-text-default) 72%, #5f8ed8);
}

.file-card span {
  display: block;
  color: var(--shell-text-muted);
  font-size: 0.78rem;
  margin-top: 4px;
}

.image-card,
.video-card {
  display: grid;
  gap: 8px;
}

.image-preview,
.video-preview {
  display: block;
  width: min(100%, 280px);
  max-height: 240px;
  object-fit: cover;
  border-radius: 14px;
  border: 0;
  background: color-mix(in srgb, var(--shell-surface-soft) 86%, #d7ebff);
}

.media-copy {
  display: grid;
  gap: 4px;
}

.media-copy span {
  color: var(--shell-text-muted);
  font-size: 0.78rem;
}

.audio-wave {
  display: grid;
  grid-template-columns: repeat(18, 1fr);
  gap: 3px;
  align-items: center;
  min-width: 128px;
}

.audio-wave span {
  height: 6px;
  border-radius: 999px;
  background: color-mix(in srgb, var(--shell-text-soft) 58%, #7ea2d6);
}

.audio-wave span:nth-child(odd) {
  height: 14px;
}

.audio-wave span:nth-child(3n) {
  height: 20px;
}

.composer {
  align-items: flex-end;
  gap: 10px;
  margin: 12px 16px 0;
  padding: 10px 12px 12px;
  border-top: 0;
  border-radius: 18px;
  background: color-mix(in srgb, var(--shell-surface-soft) 84%, white);
  flex-wrap: wrap;
}

.composer-actions {
  gap: 6px;
}

.composer-reply {
  flex: 1 1 100%;
  justify-content: space-between;
}

.composer-reply-copy {
  min-width: 0;
}

.composer-input {
  flex: 1;
}

.composer-input :deep(.p-textarea) {
  min-height: 44px;
  border-radius: 14px;
  background: rgba(255, 255, 255, 0.9);
  color: var(--shell-text-default);
  border-color: transparent;
  box-shadow: none;
}

.chat-empty {
  display: grid;
  place-items: center;
  align-content: center;
  gap: 12px;
  min-height: 100%;
  text-align: center;
}

.empty-mark {
  display: grid;
  place-items: center;
  width: 72px;
  height: 72px;
  border-radius: 999px;
  background: var(--shell-surface-soft);
  color: var(--shell-text-muted);
  font-size: 1.5rem;
}

.chat-empty h2,
.chat-empty p {
  margin: 0;
}

.chat-empty p {
  color: var(--shell-text-muted);
}
</style>
