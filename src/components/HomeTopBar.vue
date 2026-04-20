<script setup lang="ts">
import { computed } from "vue";
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import type { CircleItem, UserProfile } from "../types/chat";

const props = defineProps<{
  user: UserProfile;
  circle: CircleItem | null;
}>();

const emit = defineEmits<{
  (event: "avatar-click"): void;
  (event: "title-click"): void;
  (event: "add-click"): void;
}>();

const titleText = computed(() => {
  if (!props.circle) {
    return "Chat";
  }

  switch (props.circle.status) {
    case "connecting":
      return "Connecting";
    case "closed":
      return "Disconnected";
    default:
      return "Chat";
  }
});

const statusClass = computed(() => {
  return `status-${props.circle?.status ?? "closed"}`;
});

const addButtonLabel = computed(() => {
  return props.circle ? "New message" : "Add or restore circle";
});
</script>

<template>
  <header class="home-topbar">
    <button type="button" class="avatar-button" @click="emit('avatar-click')">
      <Avatar :label="user.initials" shape="circle" class="user-avatar" />
    </button>

    <button type="button" class="title-button" @click="emit('title-click')">
      <div class="title-copy">
        <div class="title-line">
          <span class="status-dot" :class="statusClass"></span>
          <strong>{{ titleText }}</strong>
          <i class="pi pi-chevron-down"></i>
        </div>
        <span class="subtitle">{{ circle?.name ?? "No Circle" }}</span>
      </div>
    </button>

    <Button
      icon="pi pi-plus"
      rounded
      severity="contrast"
      :aria-label="addButtonLabel"
      @click="emit('add-click')"
    />
  </header>
</template>

<style scoped>
.home-topbar {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 14px;
  padding: 16px 18px;
  border-radius: 24px;
  background: var(--shell-surface);
  border: 1px solid var(--shell-border);
  box-shadow: var(--shell-shadow-soft);
}

.avatar-button,
.title-button {
  padding: 0;
  border: 0;
  background: transparent;
}

.avatar-button {
  cursor: pointer;
}

.title-button {
  cursor: pointer;
}

.user-avatar {
  width: 40px;
  height: 40px;
  background: var(--shell-avatar-bg);
  color: var(--shell-avatar-text);
  font-weight: 700;
}

.title-copy {
  display: grid;
  gap: 2px;
  justify-items: center;
}

.title-line {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--shell-text-strong);
}

.title-line strong {
  font-size: 1.05rem;
}

.subtitle {
  color: var(--shell-text-muted);
  font-size: 0.88rem;
}

.status-dot {
  width: 10px;
  height: 10px;
  border-radius: 999px;
  background: var(--shell-border);
}

.status-open {
  background: #3bc18a;
}

.status-connecting {
  background: #f0b34d;
}

.status-closed {
  background: #d57373;
}
</style>
