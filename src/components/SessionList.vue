<script setup lang="ts">
import Avatar from "primevue/avatar";
import Badge from "primevue/badge";
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import ScrollPanel from "primevue/scrollpanel";
import Tag from "primevue/tag";
import SelfChatIcon from "./SelfChatIcon.vue";
import type { CircleItem, SessionAction, SessionItem } from "../types/chat";

defineProps<{
  searchText: string;
  sessions: SessionItem[];
  activeSessionId: string;
  activeCircle: CircleItem | null;
  archivedCount: number;
}>();

const emit = defineEmits<{
  (event: "update:searchText", value: string): void;
  (event: "select-session", sessionId: string): void;
  (event: "empty-action"): void;
  (event: "session-action", payload: { sessionId: string; action: SessionAction }): void;
  (event: "open-archived"): void;
}>();

function preview(session: SessionItem) {
  return session.draft ? `Draft: ${session.draft}` : session.subtitle;
}

function emptyCta(circle: CircleItem | null) {
  if (!circle) {
    return "Add or Restore Circle";
  }

  return circle?.type === "paid" ? "Invite to Circle" : "Add Friends to Chat";
}

function emptyDescription(circle: CircleItem | null) {
  return circle?.description ?? "No circle selected yet. Add or restore a circle before starting chats.";
}
</script>

<template>
  <section class="session-pane">
    <div class="search-row">
      <div class="search-field">
        <i class="pi pi-search"></i>
        <InputText
          :model-value="searchText"
          placeholder="Search chats"
          @update:model-value="emit('update:searchText', String($event))"
        />
      </div>
    </div>

    <ScrollPanel class="session-scroll">
      <div v-if="sessions.length" class="session-list">
        <button
          v-for="session in sessions"
          :key="session.id"
          type="button"
          :class="['session-row', { active: session.id === activeSessionId, pinned: session.pinned }]"
          @click="emit('select-session', session.id)"
        >
          <div class="session-avatar">
            <SelfChatIcon v-if="session.kind === 'self'" />
            <Avatar
              v-else
              :label="session.initials"
              shape="circle"
              class="contact-avatar"
            />
          </div>

          <div class="session-content">
            <div class="session-headline">
              <div class="session-name-row">
                <strong>{{ session.name }}</strong>
                <Tag
                  v-if="session.kind === 'group'"
                  value="Group"
                  severity="secondary"
                  rounded
                />
              </div>
              <span class="session-time">{{ session.time }}</span>
            </div>

            <div class="session-subline">
              <p :class="{ draft: !!session.draft }">{{ preview(session) }}</p>
              <Badge v-if="session.unreadCount" :value="session.unreadCount" severity="danger" />
              <span v-else-if="session.muted" class="mute-dot"></span>
            </div>
          </div>

          <div class="session-actions">
            <Button
              :icon="session.pinned ? 'pi pi-thumbtack-fill' : 'pi pi-thumbtack'"
              rounded
              text
              severity="secondary"
              @click.stop="emit('session-action', { sessionId: session.id, action: 'pin' })"
            />
            <Button
              :icon="session.muted ? 'pi pi-volume-up' : 'pi pi-volume-off'"
              rounded
              text
              severity="secondary"
              @click.stop="emit('session-action', { sessionId: session.id, action: 'mute' })"
            />
            <Button
              icon="pi pi-box"
              rounded
              text
              severity="secondary"
              @click.stop="emit('session-action', { sessionId: session.id, action: 'archive' })"
            />
            <Button
              icon="pi pi-trash"
              rounded
              text
              severity="secondary"
              @click.stop="emit('session-action', { sessionId: session.id, action: 'delete' })"
            />
          </div>
        </button>

        <button
          v-if="archivedCount > 0"
          type="button"
          class="archived-footer"
          @click="emit('open-archived')"
        >
          <span>Archived Chats</span>
          <Badge :value="archivedCount" severity="secondary" />
          <i class="pi pi-chevron-right"></i>
        </button>
      </div>

      <div v-else class="empty-state">
        <div class="empty-graphic"></div>
        <h3>Welcome to XChat</h3>
        <p>
          {{ emptyDescription(activeCircle) }}
        </p>
        <Button
          icon="pi pi-user-plus"
          :label="emptyCta(activeCircle)"
          severity="contrast"
          @click="emit('empty-action')"
        />
      </div>
    </ScrollPanel>
  </section>
</template>

<style scoped>
.session-pane {
  display: grid;
  min-height: 0;
  padding: 18px;
  border-radius: 28px;
  background: var(--shell-surface);
  border: 1px solid var(--shell-border);
  box-shadow: var(--shell-shadow-soft);
}

.search-row {
  margin-bottom: 16px;
}

.search-field {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: 10px;
  align-items: center;
  width: 100%;
  padding: 0 14px;
  border: 1px solid var(--shell-border);
  border-radius: 16px;
  background: var(--shell-surface-muted);
}

.search-field i {
  color: var(--shell-text-soft);
}

.search-field :deep(.p-inputtext) {
  border: 0;
  box-shadow: none;
  background: transparent;
  padding-left: 0;
}

.session-scroll {
  height: 100%;
}

.session-list {
  padding-right: 10px;
}

.session-row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  gap: 14px;
  width: 100%;
  padding: 14px 12px;
  border: 0;
  border-radius: 20px;
  background: transparent;
  text-align: left;
  cursor: pointer;
  transition:
    background-color 0.2s ease,
    box-shadow 0.2s ease;
}

.session-row + .session-row {
  margin-top: 8px;
}

.session-row:hover {
  background: var(--shell-hover);
}

.session-row.active {
  background: var(--shell-selected);
  box-shadow: inset 0 0 0 1px var(--shell-selected-border);
}

.session-row.pinned {
  background-color: color-mix(in srgb, var(--shell-surface) 92%, var(--shell-hover));
}

.session-content {
  display: grid;
  gap: 8px;
  min-width: 0;
}

.session-headline,
.session-subline,
.session-name-row,
.session-actions,
.archived-footer {
  display: flex;
  align-items: center;
  gap: 8px;
}

.session-headline,
.session-subline {
  justify-content: space-between;
}

.session-name-row {
  min-width: 0;
}

.session-name-row strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.session-time {
  color: var(--shell-text-soft);
  font-size: 0.82rem;
  white-space: nowrap;
}

.session-subline p {
  margin: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--shell-text-muted);
}

.session-subline p.draft {
  color: #d04f4f;
}

.contact-avatar {
  width: 44px;
  height: 44px;
  background: var(--shell-avatar-bg);
  color: var(--shell-avatar-text);
  font-weight: 700;
}

.mute-dot {
  width: 10px;
  height: 10px;
  border-radius: 999px;
  background: var(--shell-border);
}

.session-actions {
  opacity: 0;
  transition: opacity 0.18s ease;
}

.session-row:hover .session-actions,
.session-row.active .session-actions {
  opacity: 1;
}

.archived-footer {
  justify-content: center;
  width: 100%;
  margin-top: 8px;
  padding: 12px;
  border: 0;
  border-radius: 16px;
  background: transparent;
  color: var(--shell-text-default);
  cursor: pointer;
}

.archived-footer:hover {
  background: var(--shell-hover);
}

.empty-state {
  display: grid;
  justify-items: center;
  align-content: center;
  gap: 16px;
  min-height: 100%;
  padding: 28px;
  text-align: center;
}

.empty-graphic {
  width: 88px;
  height: 88px;
  border-radius: 999px;
  background:
    radial-gradient(circle at 30% 30%, rgba(111, 154, 220, 0.34) 0%, rgba(111, 154, 220, 0.34) 28%, transparent 29%),
    radial-gradient(circle at 70% 62%, rgba(97, 198, 157, 0.3) 0%, rgba(97, 198, 157, 0.3) 30%, transparent 31%),
    var(--shell-surface-soft);
}

.empty-state h3,
.empty-state p {
  margin: 0;
}

.empty-state p {
  max-width: 30ch;
  color: var(--shell-text-muted);
  line-height: 1.6;
}

@media (max-width: 720px) {
  .session-row {
    grid-template-columns: auto minmax(0, 1fr);
  }

  .session-actions {
    display: none;
  }
}
</style>
