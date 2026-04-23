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
  display: grid;
  grid-template-rows: minmax(0, 1fr);
  background: var(--shell-page-bg);
}

.page-shell-panel {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr) auto;
  width: 100vw;
  height: 100vh;
  background: var(--shell-surface-strong);
  border: 0;
  box-shadow: none;
  overflow: hidden;
  isolation: isolate;
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
  position: sticky;
  top: 0;
  z-index: 1;
  padding: max(12px, env(safe-area-inset-top)) 16px 10px;
  border-bottom: 1px solid var(--shell-border-soft);
  background: color-mix(in srgb, var(--shell-surface-strong) 94%, transparent);
  backdrop-filter: blur(18px);
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
  font-size: 1rem;
}

.page-shell-copy p {
  margin-top: 2px;
  color: var(--shell-text-muted);
  font-size: 0.84rem;
}

.page-shell-body {
  min-height: 0;
  padding: 0 16px max(18px, env(safe-area-inset-bottom));
  overflow: auto;
  overscroll-behavior: contain;
}

.page-shell-footer {
  position: sticky;
  bottom: 0;
  z-index: 1;
  padding: 12px 16px max(18px, env(safe-area-inset-bottom));
  border-top: 1px solid var(--shell-border-soft);
  background: color-mix(in srgb, var(--shell-surface-strong) 96%, transparent);
  backdrop-filter: blur(18px);
}

@media (max-width: 720px) {
  .page-shell-header {
    padding: max(10px, env(safe-area-inset-top)) 12px 8px;
  }

  .page-shell-body {
    padding: 0 12px max(14px, env(safe-area-inset-bottom));
  }

  .page-shell-footer {
    padding: 10px 12px max(14px, env(safe-area-inset-bottom));
  }
}
</style>
