<script setup lang="ts">
import { computed } from "vue";
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import Divider from "primevue/divider";
import ScrollPanel from "primevue/scrollpanel";
import Textarea from "primevue/textarea";
import SelfChatIcon from "./SelfChatIcon.vue";
import type { MessageItem, SessionItem } from "../types/chat";

const props = defineProps<{
  session: SessionItem | null;
  messages: MessageItem[];
  canLoadOlderMessages: boolean;
  loadingOlderMessages: boolean;
  composerText: string;
}>();

const emit = defineEmits<{
  (event: "load-older"): void;
  (event: "update:composerText", value: string): void;
  (event: "send"): void;
  (event: "retry-message", messageId: string): void;
  (event: "open-profile"): void;
  (event: "open-details"): void;
}>();

const subtitle = computed(() => {
  if (!props.session) {
    return "";
  }

  if (props.session.kind === "self") {
    return "Private note space";
  }

  if (props.session.kind === "group") {
    return `${props.session.members ?? 0} members · encrypted group`;
  }

  return "Direct message · end-to-end relay";
});

const showCallActions = computed(() => {
  return props.session?.kind === "direct";
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
</script>

<template>
  <section class="chat-pane">
    <template v-if="session">
      <header class="chat-header">
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
            v-if="showCallActions"
            icon="pi pi-phone"
            rounded
            text
            severity="secondary"
          />
          <Button
            v-if="showCallActions"
            icon="pi pi-video"
            rounded
            text
            severity="secondary"
          />
          <Button
            icon="pi pi-info-circle"
            rounded
            text
            severity="secondary"
            @click="emit('open-details')"
          />
        </div>
      </header>

      <Divider />

      <ScrollPanel class="message-scroll">
        <div class="message-list">
          <div v-if="canLoadOlderMessages" class="message-history-action">
            <Button
              label="Load older"
              size="small"
              text
              severity="secondary"
              :loading="loadingOlderMessages"
              @click="emit('load-older')"
            />
          </div>

          <template v-for="message in messages" :key="message.id">
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
                v-if="message.author === 'peer' && session.kind !== 'self'"
                :label="session.initials"
                shape="circle"
                class="bubble-avatar"
              />

              <div class="message-cluster">
                <div
                  :class="[
                    'message-bubble',
                    `kind-${message.kind}`,
                    {
                      mine: message.author === 'me',
                    },
                  ]"
                >
                  <template v-if="message.kind === 'text'">
                    <p>{{ message.body }}</p>
                  </template>

                  <template v-else-if="message.kind === 'file'">
                    <div class="file-card">
                      <i class="pi pi-file"></i>
                      <div>
                        <strong>{{ message.body }}</strong>
                        <span>{{ message.meta }}</span>
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
          <Button icon="pi pi-paperclip" rounded text severity="secondary" />
          <Button icon="pi pi-face-smile" rounded text severity="secondary" />
          <Button icon="pi pi-microphone" rounded text severity="secondary" />
        </div>

        <Textarea
          :model-value="composerText"
          auto-resize
          rows="1"
          class="composer-input"
          placeholder="Write a message"
          @update:model-value="emit('update:composerText', String($event))"
          @keydown.enter.exact.prevent="emit('send')"
        />

        <Button icon="pi pi-send" rounded severity="contrast" @click="emit('send')" />
      </div>
    </template>

    <div v-else class="chat-empty">
      <div class="empty-mark">
        <i class="pi pi-comment"></i>
      </div>
      <h2>Select a conversation</h2>
      <p>Choose a session on the left to open the chat page.</p>
    </div>
  </section>
</template>

<style scoped>
.chat-pane {
  display: grid;
  grid-template-rows: auto auto minmax(0, 1fr) auto;
  gap: 0;
  min-height: 0;
  padding: 20px 22px 18px;
  border-radius: 28px;
  background: var(--shell-surface);
  border: 1px solid var(--shell-border);
  box-shadow: var(--shell-shadow-soft);
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

.chat-header {
  align-items: center;
  justify-content: space-between;
  gap: 14px;
}

.chat-title {
  align-items: center;
  gap: 14px;
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
  margin-top: 4px;
  color: var(--shell-text-muted);
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
  width: 48px;
  height: 48px;
}

.bubble-avatar {
  width: 32px;
  height: 32px;
  margin-bottom: 20px;
}

.message-scroll {
  min-height: 0;
}

.message-list {
  padding-right: 10px;
}

.message-history-action {
  justify-content: center;
  margin-bottom: 10px;
}

.system-line {
  margin: 12px auto 18px;
  width: fit-content;
  padding: 8px 12px;
  border-radius: 999px;
  background: var(--shell-surface-soft);
  color: var(--shell-text-soft);
  font-size: 0.82rem;
}

.message-row {
  align-items: flex-end;
  gap: 10px;
  margin-bottom: 16px;
}

.message-row.mine {
  justify-content: flex-end;
}

.message-cluster {
  display: grid;
  gap: 6px;
  max-width: min(78%, 620px);
}

.message-row.mine .message-cluster {
  justify-items: end;
}

.message-bubble {
  border-radius: 22px;
  padding: 14px 16px;
  background: var(--shell-surface-soft);
  box-shadow: inset 0 0 0 1px var(--shell-border-soft);
}

.message-bubble.mine {
  background: var(--shell-selected);
  box-shadow: inset 0 0 0 1px var(--shell-selected-border);
}

.message-bubble p {
  margin: 0;
  line-height: 1.65;
}

.message-time {
  color: var(--shell-text-soft);
  font-size: 0.82rem;
}

.message-meta {
  align-items: center;
  gap: 8px;
}

.message-status {
  font-size: 0.78rem;
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
}

.file-card,
.audio-card {
  align-items: center;
  gap: 12px;
}

.file-card i,
.audio-card i {
  font-size: 1.2rem;
  color: color-mix(in srgb, var(--shell-text-default) 72%, #5f8ed8);
}

.file-card span {
  display: block;
  color: var(--shell-text-muted);
  font-size: 0.82rem;
  margin-top: 4px;
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
  gap: 12px;
  padding-top: 18px;
}

.composer-actions {
  gap: 6px;
}

.composer-input {
  flex: 1;
}

.composer-input :deep(.p-textarea) {
  min-height: 52px;
  border-radius: 18px;
  background: var(--shell-surface-muted);
  color: var(--shell-text-default);
  border-color: var(--shell-border);
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
