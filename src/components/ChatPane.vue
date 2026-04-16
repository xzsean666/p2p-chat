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
  composerText: string;
}>();

const emit = defineEmits<{
  (event: "update:composerText", value: string): void;
  (event: "send"): void;
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
                <span class="message-time">{{ message.time }}</span>
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
  background: rgba(255, 255, 255, 0.92);
  border: 1px solid rgba(210, 220, 232, 0.9);
  box-shadow: 0 20px 50px rgba(24, 46, 84, 0.08);
}

.chat-header,
.chat-title,
.chat-actions,
.composer,
.composer-actions,
.message-row,
.file-card,
.audio-card {
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
  color: #68788f;
}

.chat-actions {
  gap: 8px;
}

.contact-avatar,
.bubble-avatar {
  background: linear-gradient(135deg, #dce9ff 0%, #d9f9ef 100%);
  color: #1c355d;
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

.system-line {
  margin: 12px auto 18px;
  width: fit-content;
  padding: 8px 12px;
  border-radius: 999px;
  background: #f3f7fb;
  color: #7a8ca3;
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
  background: #f4f7fb;
  box-shadow: inset 0 0 0 1px rgba(218, 228, 240, 0.9);
}

.message-bubble.mine {
  background: linear-gradient(135deg, #dff4ea 0%, #e7f7ff 100%);
  box-shadow: inset 0 0 0 1px rgba(176, 219, 205, 0.95);
}

.message-bubble p {
  margin: 0;
  line-height: 1.65;
}

.message-time {
  color: #7a8ca3;
  font-size: 0.82rem;
}

.file-card,
.audio-card {
  align-items: center;
  gap: 12px;
}

.file-card i,
.audio-card i {
  font-size: 1.2rem;
  color: #496c9b;
}

.file-card span {
  display: block;
  color: #72839d;
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
  background: #7ea2d6;
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
  background: #f6f9fc;
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
  background: #f3f7fb;
  color: #6c84a4;
  font-size: 1.5rem;
}

.chat-empty h2,
.chat-empty p {
  margin: 0;
}

.chat-empty p {
  color: #697b93;
}
</style>
