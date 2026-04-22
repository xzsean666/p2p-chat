<script setup lang="ts">
import Button from "primevue/button";
import Tag from "primevue/tag";
import OverlayPageShell from "./OverlayPageShell.vue";
import SelfChatIcon from "./SelfChatIcon.vue";
import type { CircleItem } from "../types/chat";

defineProps<{
  circle: CircleItem | null;
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "confirm"): void;
}>();
</script>

<template>
  <OverlayPageShell
    title="File Transfer Assistant"
    subtitle="Open the private assistant chat for the current circle."
    @close="emit('close')"
  >
    <div class="confirm-body">
      <section class="hero-card">
        <SelfChatIcon size="lg" />
        <div class="hero-copy">
          <h3>{{ circle?.name ?? "No Active Circle" }}</h3>
          <p>
            {{
              circle
                ? "This chat stays scoped to this circle, so notes, links and files stay separate from other circles."
                : "Choose a circle first, then open File Transfer Assistant for that circle."
            }}
          </p>
        </div>
        <Tag
          v-if="circle"
          :value="circle.status"
          :severity="circle.status === 'open' ? 'success' : circle.status === 'connecting' ? 'warn' : 'secondary'"
          rounded
        />
      </section>

      <section class="section-card">
        <div class="section-title">What Opens</div>
        <div class="info-list">
          <div class="info-row">
            <strong>Assistant Chat</strong>
            <p>Create or reopen the `File Transfer Assistant` chat for this circle.</p>
          </div>
          <div class="info-row">
            <strong>Circle Scoped</strong>
            <p>Messages and saved items stay attached to the current circle.</p>
          </div>
          <div class="info-row">
            <strong>Quick Saves</strong>
            <p>Use it for notes, links and files you want close at hand.</p>
          </div>
        </div>
      </section>
    </div>

    <template #footer>
      <div class="footer-actions">
        <Button label="Cancel" text severity="secondary" @click="emit('close')" />
        <Button
          icon="pi pi-arrow-right"
          label="Open File Transfer Assistant"
          severity="contrast"
          :disabled="!circle"
          @click="emit('confirm')"
        />
      </div>
    </template>
  </OverlayPageShell>
</template>

<style scoped>
.confirm-body,
.hero-card,
.hero-copy,
.section-card,
.info-list {
  display: grid;
}

.confirm-body {
  gap: 18px;
}

.hero-card,
.section-card {
  gap: 14px;
  padding: 24px;
  border-radius: 28px;
}

.hero-card {
  justify-items: center;
  text-align: center;
  background:
    radial-gradient(circle at top left, rgba(106, 168, 255, 0.18), transparent 26%),
    linear-gradient(180deg, #f7fbfe 0%, #f2f7fb 100%);
}

.hero-copy {
  gap: 8px;
}

.section-card {
  background: #f8fbfd;
}

.section-title {
  color: #6a7d98;
  text-transform: uppercase;
  letter-spacing: 0.14em;
  font-size: 0.72rem;
}

.info-list {
  gap: 10px;
}

.info-row {
  display: grid;
  gap: 6px;
  padding: 16px 18px;
  border-radius: 20px;
  background: #ffffff;
}

.hero-copy h3,
.hero-copy p,
.info-row strong,
.info-row p {
  margin: 0;
}

.hero-copy p,
.info-row p {
  color: #6d809a;
  line-height: 1.65;
}

.footer-actions {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
}
</style>
