<script setup lang="ts">
import { computed } from "vue";
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import ScrollPanel from "primevue/scrollpanel";
import Tag from "primevue/tag";
import type { CircleItem } from "../types/chat";

const props = defineProps<{
  circles: CircleItem[];
  activeCircleId: string;
}>();

const emit = defineEmits<{
  (event: "select", circleId: string): void;
  (event: "join"): void;
  (event: "restore"): void;
}>();

const titleText = computed(() => {
  return props.circles.length ? "Switch Circle" : "Add or Restore Circle";
});

function tone(status: CircleItem["status"]) {
  if (status === "open") {
    return "success";
  }

  if (status === "connecting") {
    return "warn";
  }

  return "danger";
}

function label(status: CircleItem["status"]) {
  if (status === "open") {
    return "Connected";
  }

  if (status === "connecting") {
    return "Connecting";
  }

  return "Disconnected";
}
</script>

<template>
  <section class="switcher-card">
    <header class="switcher-header">
      <div>
        <p class="eyebrow">Circles</p>
        <h2>{{ titleText }}</h2>
      </div>
      <Tag value="XChat Style" severity="secondary" rounded />
    </header>

    <ScrollPanel v-if="circles.length" class="switcher-scroll">
      <button
        v-for="circle in circles"
        :key="circle.id"
        type="button"
        :class="['circle-row', { active: circle.id === activeCircleId }]"
        @click="emit('select', circle.id)"
      >
        <Avatar :label="circle.name.slice(0, 1)" shape="circle" class="circle-avatar" />
        <div class="circle-copy">
          <div class="row-head">
            <strong>{{ circle.name }}</strong>
            <i
              v-if="circle.id === activeCircleId"
              class="pi pi-check-circle active-check"
            ></i>
          </div>
          <p>{{ circle.description }}</p>
          <div class="row-meta">
            <Tag :value="label(circle.status)" :severity="tone(circle.status)" rounded />
            <span>{{ circle.latency }}</span>
            <span>{{ circle.relay }}</span>
          </div>
        </div>
      </button>
    </ScrollPanel>

    <div v-else class="switcher-empty">
      <div class="empty-mark">
        <i class="pi pi-compass"></i>
      </div>
      <h3>No circles in this shell</h3>
      <p>Join a new circle now, or open the local restore catalog for a previously removed one.</p>
    </div>

    <div class="switcher-actions">
      <Button
        icon="pi pi-plus"
        :label="circles.length ? 'Add a Circle' : 'Join Circle'"
        severity="contrast"
        @click="emit('join')"
      />
      <Button
        v-if="!circles.length"
        icon="pi pi-history"
        label="Restore Access"
        severity="secondary"
        outlined
        @click="emit('restore')"
      />
    </div>
  </section>
</template>

<style scoped>
.switcher-card {
  display: grid;
  gap: 16px;
  width: min(620px, calc(100vw - 32px));
  max-height: min(78vh, 680px);
  padding: 22px;
  border-radius: 28px;
  background: rgba(255, 255, 255, 0.96);
  border: 1px solid rgba(210, 220, 232, 0.92);
  box-shadow: 0 28px 70px rgba(24, 46, 84, 0.14);
}

.switcher-header {
  display: flex;
  align-items: start;
  justify-content: space-between;
  gap: 14px;
}

.eyebrow,
h2,
p {
  margin: 0;
}

.eyebrow {
  color: #667a97;
  text-transform: uppercase;
  letter-spacing: 0.16em;
  font-size: 0.76rem;
}

h2 {
  margin-top: 4px;
  font-size: 1.35rem;
}

.switcher-scroll {
  min-height: 0;
}

.switcher-empty {
  display: grid;
  gap: 12px;
  place-items: center;
  min-height: 220px;
  padding: 24px;
  border-radius: 22px;
  background: linear-gradient(180deg, rgba(244, 248, 252, 0.96) 0%, rgba(239, 245, 250, 0.96) 100%);
  border: 1px dashed rgba(176, 191, 209, 0.92);
  text-align: center;
}

.empty-mark {
  display: grid;
  place-items: center;
  width: 58px;
  height: 58px;
  border-radius: 999px;
  background: linear-gradient(135deg, #dce9ff 0%, #d9f9ef 100%);
  color: #1f4d7a;
  font-size: 1.35rem;
}

.switcher-empty h3 {
  font-size: 1.08rem;
  color: #17385e;
}

.switcher-empty p {
  max-width: 340px;
  color: #61748f;
  line-height: 1.6;
}

.switcher-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
}

.circle-row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: 14px;
  width: 100%;
  padding: 14px 12px;
  border: 0;
  border-radius: 18px;
  background: transparent;
  text-align: left;
  cursor: pointer;
}

.circle-row + .circle-row {
  margin-top: 8px;
}

.circle-row:hover {
  background: rgba(243, 247, 252, 0.95);
}

.circle-row.active {
  background: linear-gradient(135deg, #eff5ff 0%, #eefaf5 100%);
  box-shadow: inset 0 0 0 1px rgba(170, 198, 228, 0.92);
}

.circle-avatar {
  width: 42px;
  height: 42px;
  background: linear-gradient(135deg, #dce9ff 0%, #d9f9ef 100%);
  color: #16355c;
  font-weight: 700;
}

.circle-copy {
  display: grid;
  gap: 8px;
  min-width: 0;
}

.row-head,
.row-meta {
  display: flex;
  align-items: center;
  gap: 10px;
}

.row-head {
  justify-content: space-between;
}

.circle-copy strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.circle-copy p {
  color: #61748f;
  line-height: 1.55;
}

.row-meta {
  flex-wrap: wrap;
  color: #7b8ca4;
  font-size: 0.86rem;
}

.active-check {
  color: #2d8f68;
}
</style>
