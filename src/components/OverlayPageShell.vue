<script setup lang="ts">
import Button from "primevue/button";

defineProps<{
  title: string;
  subtitle?: string;
}>();

const emit = defineEmits<{
  (event: "close"): void;
}>();
</script>

<template>
  <div class="page-shell-layer">
    <div class="page-shell-mask" @click="emit('close')"></div>

    <section class="page-shell-panel">
      <header class="page-shell-header">
        <div class="page-shell-leading">
          <Button
            icon="pi pi-arrow-left"
            rounded
            text
            severity="secondary"
            aria-label="Back"
            @click="emit('close')"
          />

          <div class="page-shell-copy">
            <h2>{{ title }}</h2>
            <p v-if="subtitle">{{ subtitle }}</p>
          </div>
        </div>

        <div v-if="$slots.actions" class="page-shell-actions">
          <slot name="actions" />
        </div>
      </header>

      <div class="page-shell-body">
        <slot />
      </div>

      <footer v-if="$slots.footer" class="page-shell-footer">
        <slot name="footer" />
      </footer>
    </section>
  </div>
</template>

<style scoped>
.page-shell-layer {
  position: fixed;
  inset: 0;
  z-index: 34;
  display: flex;
  justify-content: flex-end;
  padding: 18px;
}

.page-shell-mask {
  position: absolute;
  inset: 0;
  background: rgba(15, 23, 42, 0.28);
  backdrop-filter: blur(6px);
}

.page-shell-panel {
  position: relative;
  display: grid;
  grid-template-rows: auto minmax(0, 1fr) auto;
  width: min(760px, calc(100vw - 36px));
  height: calc(100vh - 36px);
  border-radius: 30px;
  background: rgba(255, 255, 255, 0.97);
  border: 1px solid rgba(210, 220, 232, 0.92);
  box-shadow: 0 26px 64px rgba(17, 36, 66, 0.2);
  overflow: hidden;
}

.page-shell-header,
.page-shell-leading,
.page-shell-actions {
  display: flex;
  align-items: center;
}

.page-shell-header {
  justify-content: space-between;
  gap: 16px;
  padding: 18px 20px 14px;
  border-bottom: 1px solid rgba(224, 232, 240, 0.9);
  background: rgba(251, 253, 255, 0.92);
}

.page-shell-leading {
  gap: 12px;
  min-width: 0;
}

.page-shell-actions {
  gap: 6px;
}

.page-shell-copy {
  min-width: 0;
}

.page-shell-copy h2,
.page-shell-copy p {
  margin: 0;
}

.page-shell-copy h2 {
  font-size: 1.05rem;
}

.page-shell-copy p {
  margin-top: 4px;
  color: #71839d;
  font-size: 0.9rem;
}

.page-shell-body {
  min-height: 0;
  padding: 22px 22px 18px;
  overflow: auto;
}

.page-shell-footer {
  padding: 16px 22px 22px;
  border-top: 1px solid rgba(224, 232, 240, 0.9);
  background: rgba(251, 253, 255, 0.94);
}

@media (max-width: 920px) {
  .page-shell-layer {
    padding: 12px;
  }

  .page-shell-panel {
    width: calc(100vw - 24px);
    height: calc(100vh - 24px);
  }
}

@media (max-width: 720px) {
  .page-shell-layer {
    padding: 0;
  }

  .page-shell-panel {
    width: 100vw;
    height: 100vh;
    border-radius: 0;
    border: 0;
  }

  .page-shell-header {
    padding-top: max(18px, env(safe-area-inset-top));
  }

  .page-shell-footer {
    padding-bottom: max(18px, env(safe-area-inset-bottom));
  }
}
</style>
