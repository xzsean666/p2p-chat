<script setup lang="ts">
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import SelfChatIcon from "./SelfChatIcon.vue";
import OverlayPageShell from "./OverlayPageShell.vue";
import type { SessionItem } from "../types/chat";

defineProps<{
  sessions: SessionItem[];
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "open-session", sessionId: string): void;
  (event: "unarchive-session", sessionId: string): void;
}>();
</script>

<template>
  <OverlayPageShell
    title="Archived Chats"
    subtitle="Restore archived conversations or reopen them directly."
    @close="emit('close')"
  >
    <div v-if="sessions.length" class="archived-list">
      <div v-for="session in sessions" :key="session.id" class="archived-row">
        <button type="button" class="archived-main" @click="emit('open-session', session.id)">
          <div class="archived-avatar">
            <SelfChatIcon v-if="session.kind === 'self'" />
            <Avatar
              v-else
              :label="session.initials"
              shape="circle"
              class="contact-avatar"
            />
          </div>

          <div class="archived-copy">
            <div class="row-head">
              <strong>{{ session.name }}</strong>
              <span>{{ session.time }}</span>
            </div>
            <p>{{ session.subtitle }}</p>
          </div>
        </button>

        <div class="archived-actions">
          <Button
            icon="pi pi-folder-open"
            rounded
            text
            severity="secondary"
            aria-label="Unarchive"
            @click="emit('unarchive-session', session.id)"
          />
          <Button
            icon="pi pi-arrow-right"
            rounded
            text
            severity="secondary"
            aria-label="Open conversation"
            @click="emit('open-session', session.id)"
          />
        </div>
      </div>
    </div>

    <div v-else class="empty-state">
      <div class="empty-mark">
        <i class="pi pi-inbox"></i>
      </div>
      <h3>No Archived Chats</h3>
      <p>Archived conversations will appear here when you move them out of the main session list.</p>
    </div>
  </OverlayPageShell>
</template>

<style scoped>
.archived-list {
  display: grid;
  gap: 10px;
}

.archived-row,
.archived-main,
.archived-actions,
.row-head {
  display: flex;
  align-items: center;
}

.archived-row {
  gap: 12px;
  padding: 10px 12px;
  border-radius: 20px;
  background: #f7fafc;
  border: 1px solid transparent;
}

.archived-row:hover {
  border-color: rgba(186, 202, 221, 0.9);
  background: #f2f7fb;
}

.archived-main {
  flex: 1;
  gap: 14px;
  padding: 0;
  border: 0;
  background: transparent;
  text-align: left;
  cursor: pointer;
}

.archived-avatar {
  flex: none;
}

.contact-avatar {
  width: 46px;
  height: 46px;
  background: linear-gradient(135deg, #dce9ff 0%, #d9f9ef 100%);
  color: #16355c;
  font-weight: 700;
}

.archived-copy {
  display: grid;
  gap: 6px;
  min-width: 0;
  flex: 1;
}

.row-head {
  justify-content: space-between;
  gap: 12px;
}

.row-head strong,
.archived-copy p,
.empty-state h3,
.empty-state p {
  margin: 0;
}

.row-head strong,
.archived-copy p {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.row-head span,
.archived-copy p {
  color: #6d809a;
}

.row-head span {
  flex: none;
  font-size: 0.84rem;
}

.archived-actions {
  gap: 4px;
}

.empty-state {
  display: grid;
  justify-items: center;
  align-content: center;
  gap: 12px;
  min-height: 100%;
  padding: 40px 24px 20px;
  text-align: center;
}

.empty-mark {
  display: grid;
  place-items: center;
  width: 108px;
  height: 108px;
  border-radius: 999px;
  background: linear-gradient(180deg, #eef5ff 0%, #eff9f5 100%);
  color: #6082b4;
  font-size: 2.5rem;
}

.empty-state p {
  max-width: 40ch;
  color: #6d809a;
  line-height: 1.65;
}
</style>
