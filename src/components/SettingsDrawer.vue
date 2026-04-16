<script setup lang="ts">
import { computed } from "vue";
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import Drawer from "primevue/drawer";
import Tag from "primevue/tag";
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
  sections: SettingSection[];
  phase?: string;
  showLogout?: boolean;
}>();

const emit = defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "select-circle", circleId: string): void;
  (event: "join-circle"): void;
  (event: "open-circle-detail", circleId: string): void;
  (event: "item-click", itemId: SettingPageId): void;
  (event: "logout"): void;
}>();

const drawerVisible = computed({
  get: () => props.visible,
  set: (value: boolean) => emit("update:visible", value),
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
        <span>Settings</span>
      </div>
    </template>

    <div class="drawer-body">
      <section class="profile-card">
        <Avatar :label="user.initials" shape="circle" class="profile-avatar" />
        <div>
          <strong>{{ user.name }}</strong>
          <p>{{ user.handle }}</p>
        </div>
        <Tag :value="phase || 'Foundation'" severity="info" rounded />
      </section>

      <section class="drawer-section">
        <p class="section-title">Circles</p>
        <div
          v-for="circle in circles"
          :key="circle.id"
          :class="['circle-row-shell', { active: circle.id === activeCircleId }]"
        >
          <button type="button" class="drawer-row circle-row-main" @click="emit('select-circle', circle.id)">
            <Avatar :label="circle.name.slice(0, 1)" shape="circle" class="circle-avatar" />
            <div class="row-copy">
              <strong>{{ circle.name }}</strong>
              <span>{{ circle.relay }}</span>
            </div>
            <i
              v-if="circle.id === activeCircleId"
              class="pi pi-check-circle active-check"
            ></i>
          </button>

          <Button
            icon="pi pi-info-circle"
            text
            rounded
            severity="secondary"
            class="detail-button"
            @click="emit('open-circle-detail', circle.id)"
          />
        </div>

        <Button
          icon="pi pi-plus"
          label="Add a Circle"
          text
          severity="contrast"
          class="join-button"
          @click="emit('join-circle')"
        />
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
}

.drawer-body {
  display: grid;
  gap: 20px;
}

.profile-card,
.drawer-row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.circle-row-shell {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 8px;
  align-items: center;
  border-radius: 16px;
}

.profile-card {
  padding: 14px;
  border-radius: 20px;
  background: #f5f8fb;
}

.profile-avatar,
.circle-avatar {
  background: linear-gradient(135deg, #dce9ff 0%, #d9f9ef 100%);
  color: #16355c;
  font-weight: 700;
}

.profile-avatar {
  width: 42px;
  height: 42px;
}

.circle-avatar {
  width: 34px;
  height: 34px;
}

.profile-card strong,
.profile-card p,
.section-title {
  margin: 0;
}

.profile-card p {
  color: #6f8199;
  font-size: 0.88rem;
}

.drawer-section {
  display: grid;
  gap: 8px;
}

.section-title {
  color: #6a7d98;
  text-transform: uppercase;
  letter-spacing: 0.16em;
  font-size: 0.72rem;
}

.drawer-row {
  width: 100%;
  padding: 12px 10px;
  border: 0;
  border-radius: 16px;
  background: transparent;
  cursor: pointer;
  text-align: left;
}

.drawer-row:hover,
.circle-row-shell:hover {
  background: #f4f8fb;
}

.drawer-row.active,
.circle-row-shell.active {
  background: linear-gradient(135deg, #eff5ff 0%, #eefaf5 100%);
}

.circle-row-main {
  min-width: 0;
}

.circle-row-shell .circle-row-main:hover,
.circle-row-shell.active .circle-row-main:hover {
  background: transparent;
}

.row-copy {
  display: grid;
  gap: 4px;
  min-width: 0;
  flex: 1;
}

.row-copy strong,
.row-copy span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.row-copy span {
  color: #7487a0;
  font-size: 0.84rem;
}

.active-check {
  color: #2f8c6a;
}

.detail-button {
  align-self: stretch;
}

.row-chevron {
  margin-left: auto;
  color: #8a9ab0;
}

.join-button {
  justify-content: flex-start;
  padding-left: 8px;
}

.logout-button {
  justify-content: flex-start;
  padding-left: 8px;
}

:deep(.settings-drawer) {
  width: min(420px, 100vw);
}
</style>
