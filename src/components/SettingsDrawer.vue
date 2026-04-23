<script setup lang="ts">
import { computed } from "vue";
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import Drawer from "primevue/drawer";
import type {
  CircleItem,
  SettingPageId,
  SettingSection,
  UserProfile,
} from "../types/chat";

const props = defineProps<{
  visible: boolean;
  user: UserProfile;
  circles: CircleItem[];
  activeCircleId: string;
  restorableCount: number;
  sections: SettingSection[];
  phase?: string;
  showLogout?: boolean;
}>();

const emit = defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "select-circle", circleId: string): void;
  (event: "join-circle"): void;
  (event: "open-restore"): void;
  (event: "open-circle-detail", circleId: string): void;
  (event: "item-click", itemId: SettingPageId): void;
  (event: "logout"): void;
}>();

const drawerVisible = computed({
  get: () => props.visible,
  set: (value: boolean) => emit("update:visible", value),
});

const currentCircle = computed(() => {
  return props.circles.find((circle) => circle.id === props.activeCircleId) ?? null;
});

const inactiveCircles = computed(() => {
  return props.circles.filter((circle) => circle.id !== props.activeCircleId);
});

const circleEmptyTitle = computed(() => {
  return props.restorableCount > 0 ? "Add or Restore Circle" : "Join a Circle";
});

const circleEmptyCopy = computed(() => {
  return props.restorableCount > 0
    ? "No active circle is mounted in this shell. Join a new one or reopen a saved entry from the local restore catalog."
    : "No circles are mounted in this shell yet. Join a circle before starting chats.";
});
</script>

<template>
  <Drawer
    v-model:visible="drawerVisible"
    position="left"
    class="settings-drawer"
    :dismissable="true"
    :modal="true"
  >
    <template #header>
      <div class="drawer-header">
        <span>Me</span>
      </div>
    </template>

    <div class="drawer-body">
      <section class="profile-hero">
        <Avatar :label="user.initials" shape="circle" class="profile-avatar" />
        <div class="profile-copy">
          <strong>{{ user.name }}</strong>
          <p>{{ user.handle }}</p>
          <span>{{ phase || "Foundation" }}</span>
        </div>
      </section>

      <section class="drawer-section circle-section">
        <div class="section-heading">
          <p class="section-title">Current circle</p>
          <button type="button" class="text-action" @click="emit('join-circle')">
            Add
          </button>
        </div>

        <template v-if="currentCircle">
          <button
            type="button"
            class="circle-focus-card"
            @click="emit('open-circle-detail', currentCircle.id)"
          >
            <Avatar
              :label="currentCircle.name.slice(0, 1)"
              shape="circle"
              class="circle-avatar"
            />
            <div class="row-copy">
              <strong>{{ currentCircle.name }}</strong>
              <span>{{ currentCircle.relay }}</span>
            </div>
            <span class="circle-badge">Active</span>
            <i class="pi pi-chevron-right row-chevron"></i>
          </button>

          <div v-if="inactiveCircles.length" class="circle-list">
            <p class="section-caption">Switch circle</p>
            <button
              v-for="circle in inactiveCircles"
              :key="circle.id"
              type="button"
              class="drawer-row"
              @click="emit('select-circle', circle.id)"
            >
              <Avatar
                :label="circle.name.slice(0, 1)"
                shape="circle"
                class="circle-avatar"
              />
              <div class="row-copy">
                <strong>{{ circle.name }}</strong>
                <span>{{ circle.relay }}</span>
              </div>
              <i class="pi pi-chevron-right row-chevron"></i>
            </button>
          </div>

          <div v-if="restorableCount > 0" class="inline-actions">
            <button
              type="button"
              class="text-action"
              @click="emit('open-restore')"
            >
              Restore access
            </button>
          </div>
        </template>

        <div v-else class="circle-empty-state">
          <strong>{{ circleEmptyTitle }}</strong>
          <p>{{ circleEmptyCopy }}</p>
          <div class="inline-actions">
            <button type="button" class="text-action" @click="emit('join-circle')">
              Join circle
            </button>
            <button
              v-if="restorableCount > 0"
              type="button"
              class="text-action"
              @click="emit('open-restore')"
            >
              Restore access
            </button>
          </div>
        </div>
      </section>

      <section
        v-for="section in sections"
        :key="section.title"
        class="drawer-section"
      >
        <p class="section-title">{{ section.title }}</p>
        <button
          v-for="item in section.items"
          :key="item.id"
          type="button"
          class="drawer-row"
          @click="emit('item-click', item.id)"
        >
          <i :class="item.icon"></i>
          <span>{{ item.label }}</span>
          <i class="pi pi-chevron-right row-chevron"></i>
        </button>
      </section>

      <section v-if="showLogout" class="drawer-section">
        <Button
          icon="pi pi-sign-out"
          label="Log Out"
          severity="danger"
          text
          class="logout-button"
          @click="emit('logout')"
        />
      </section>
    </div>
  </Drawer>
</template>

<style scoped>
.drawer-header {
  font-weight: 700;
  letter-spacing: 0.01em;
}

.drawer-body {
  display: grid;
  gap: 22px;
}

.profile-hero,
.circle-focus-card,
.drawer-row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.profile-hero {
  padding: 4px 2px 0;
}

.profile-avatar,
.circle-avatar {
  background: var(--shell-avatar-bg);
  color: var(--shell-avatar-text);
  font-weight: 700;
}

.profile-avatar {
  width: 48px;
  height: 48px;
}

.circle-avatar {
  width: 34px;
  height: 34px;
}

.profile-copy,
.row-copy,
.circle-empty-state,
.drawer-section {
  min-width: 0;
}

.profile-copy {
  display: grid;
  gap: 3px;
}

.profile-hero strong,
.profile-hero p,
.profile-hero span,
.section-title {
  margin: 0;
}

.profile-hero p,
.profile-hero span {
  color: var(--shell-text-muted);
  font-size: 0.88rem;
}

.profile-hero span {
  font-size: 0.8rem;
}

.drawer-section {
  display: grid;
  gap: 10px;
}

.circle-empty-state {
  display: grid;
  gap: 8px;
  padding: 16px;
  border-radius: 20px;
  background: var(--shell-surface-soft);
  border: 1px solid var(--shell-border);
}

.section-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.section-title {
  color: var(--shell-text-muted);
  text-transform: uppercase;
  letter-spacing: 0.16em;
  font-size: 0.72rem;
}

.section-caption {
  margin: 0 0 2px;
  color: var(--shell-text-muted);
  font-size: 0.8rem;
}

.circle-section {
  gap: 12px;
}

.circle-focus-card {
  width: 100%;
  padding: 14px 12px;
  border: 1px solid var(--shell-border);
  border-radius: 20px;
  background: linear-gradient(
    180deg,
    color-mix(in srgb, var(--shell-surface-soft) 88%, white 12%),
    var(--shell-surface-soft)
  );
  cursor: pointer;
  text-align: left;
}

.circle-focus-card:hover,
.drawer-row {
  width: 100%;
  padding: 12px 4px;
  border: 0;
  border-radius: 14px;
  background: transparent;
  cursor: pointer;
  text-align: left;
}

.drawer-row:hover,
.circle-focus-card:hover {
  background: var(--shell-hover);
}

.row-copy {
  display: grid;
  gap: 4px;
  flex: 1;
}

.row-copy strong,
.row-copy span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.row-copy span {
  color: var(--shell-text-soft);
  font-size: 0.84rem;
}

.circle-list {
  display: grid;
  gap: 4px;
}

.circle-badge {
  padding: 4px 9px;
  border-radius: 999px;
  background: color-mix(in srgb, var(--shell-accent, #2f8c6a) 14%, white 86%);
  color: var(--shell-accent, #2f8c6a);
  font-size: 0.74rem;
  font-weight: 600;
}

.row-chevron {
  margin-left: auto;
  color: #8a9ab0;
}

.inline-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 14px;
}

.text-action {
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--shell-accent, #2f8c6a);
  font-size: 0.9rem;
  font-weight: 600;
  cursor: pointer;
}

.text-action:hover {
  color: color-mix(in srgb, var(--shell-accent, #2f8c6a) 82%, black 18%);
}

.circle-empty-state strong,
.circle-empty-state p {
  margin: 0;
}

.circle-empty-state p {
  color: var(--shell-text-muted);
  font-size: 0.88rem;
  line-height: 1.5;
}

.logout-button {
  justify-content: flex-start;
  padding-left: 0;
}

:deep(.settings-drawer) {
  width: min(420px, 100vw);
}
</style>
