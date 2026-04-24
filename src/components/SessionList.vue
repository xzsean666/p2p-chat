<script setup lang="ts">
import { computed, ref } from "vue";
import Avatar from "primevue/avatar";
import Badge from "primevue/badge";
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import Menu from "primevue/menu";
import ScrollPanel from "primevue/scrollpanel";
import type { MenuItem } from "primevue/menuitem";
import SelfChatIcon from "./SelfChatIcon.vue";
import emptyStateImage from "../assets/empty-state.svg";
import type { CircleItem, SessionAction, SessionItem } from "../types/chat";

const props = defineProps<{
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

const sessionActionMenu = ref<{
  show: (event: Event) => void;
  hide: () => void;
} | null>(null);
const activeActionSessionId = ref<string | null>(null);

const activeActionSession = computed(() => {
  if (!activeActionSessionId.value) {
    return null;
  }

  return props.sessions.find((session) => session.id === activeActionSessionId.value) ?? null;
});

const sessionActionItems = computed<MenuItem[]>(() => {
  const session = activeActionSession.value;
  if (!session) {
    return [];
  }

  return [
    {
      label: session.pinned ? "Unpin chat" : "Pin chat",
      icon: session.pinned ? "pi pi-thumbtack" : "pi pi-thumbtack",
      command: () => emit("session-action", { sessionId: session.id, action: "pin" }),
    },
    {
      label: session.muted ? "Unmute chat" : "Mute chat",
      icon: session.muted ? "pi pi-volume-up" : "pi pi-volume-off",
      command: () => emit("session-action", { sessionId: session.id, action: "mute" }),
    },
    {
      label: "Archive chat",
      icon: "pi pi-box",
      command: () => emit("session-action", { sessionId: session.id, action: "archive" }),
    },
    {
      label: "Delete chat",
      icon: "pi pi-trash",
      command: () => emit("session-action", { sessionId: session.id, action: "delete" }),
    },
  ];
});

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

function openSessionActionMenu(event: Event, sessionId: string) {
  activeActionSessionId.value = sessionId;
  sessionActionMenu.value?.show(event);
}
</script>

<template>
  <section class="session-pane">
    <Menu
      ref="sessionActionMenu"
      :model="sessionActionItems"
      popup
      @hide="activeActionSessionId = null"
    />

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
              </div>
            </div>

            <div class="session-subline">
              <p :class="{ draft: !!session.draft }">{{ preview(session) }}</p>
            </div>
          </div>

          <div class="session-meta">
            <span class="session-time">{{ session.time }}</span>
            <Badge v-if="session.unreadCount" :value="session.unreadCount" severity="danger" />
            <Button
              icon="pi pi-ellipsis-h"
              rounded
              text
              severity="secondary"
              :class="['session-more', { visible: session.id === activeSessionId }]"
              aria-label="Open chat actions"
              @click.stop="openSessionActionMenu($event, session.id)"
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
        <img :src="emptyStateImage" alt="" class="empty-graphic" />
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
  padding: 0;
  border-radius: 0;
  background: transparent;
  border: 0;
  box-shadow: none;
  overflow: hidden;
}

.search-row {
  padding: 2px 14px 10px;
}

.search-field {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: 8px;
  align-items: center;
  width: 100%;
  min-height: 40px;
  padding: 0 10px;
  border: 0;
  border-radius: 12px;
  background: color-mix(in srgb, var(--shell-surface-soft) 82%, white);
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
  padding: 0 0 6px;
}

.session-row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  gap: 10px;
  width: 100%;
  padding: 11px 14px;
  border: 0;
  background: transparent;
  border-bottom: 1px solid color-mix(in srgb, var(--shell-border-soft) 78%, transparent);
  text-align: left;
  cursor: pointer;
  transition:
    background-color 0.2s ease,
    opacity 0.2s ease,
    border-color 0.2s ease;
}

.session-row:hover {
  background: color-mix(in srgb, var(--shell-hover) 56%, transparent);
}

.session-row.active {
  background: color-mix(in srgb, var(--shell-selected) 52%, transparent);
  border-bottom-color: transparent;
}

.session-row.pinned:not(.active) {
  background: color-mix(in srgb, var(--shell-surface-soft) 54%, transparent);
}

.session-content {
  display: grid;
  gap: 3px;
  min-width: 0;
}

.session-headline,
.session-subline,
.session-name-row,
.session-meta,
.archived-footer {
  display: flex;
  align-items: center;
  gap: 8px;
}

.session-name-row {
  min-width: 0;
}

.session-name-row strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.97rem;
  font-weight: 600;
}

.session-time {
  color: var(--shell-text-soft);
  font-size: 0.74rem;
  white-space: nowrap;
}

.session-meta {
  align-self: start;
  padding-top: 1px;
  color: var(--shell-text-soft);
}

.session-subline p {
  margin: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--shell-text-muted);
  font-size: 0.84rem;
  line-height: 1.35;
}

.session-subline p.draft {
  color: #d04f4f;
}

.contact-avatar {
  width: 38px;
  height: 38px;
  background: var(--shell-avatar-bg);
  color: var(--shell-avatar-text);
  font-weight: 700;
}

.session-more {
  width: 1.75rem;
  height: 1.75rem;
  color: var(--shell-text-soft);
  opacity: 0;
  pointer-events: none;
  transition: opacity 0.18s ease;
}

.session-row:hover .session-more,
.session-row.active .session-more,
.session-more.visible {
  opacity: 1;
  pointer-events: auto;
}

.archived-footer {
  justify-content: space-between;
  width: 100%;
  padding: 13px 14px 10px;
  border: 0;
  border-radius: 0;
  border-top: 0;
  background: transparent;
  color: var(--shell-text-default);
  cursor: pointer;
  font-size: 0.92rem;
}

.archived-footer:hover {
  background: color-mix(in srgb, var(--shell-hover) 48%, transparent);
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
  width: 104px;
  height: 104px;
  object-fit: contain;
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
}
</style>
