<script setup lang="ts">
import { computed, ref, watch } from "vue";
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import OverlayPageShell from "./OverlayPageShell.vue";
import type { GroupProfile, SessionItem } from "../types/chat";

const props = defineProps<{
  session: SessionItem | null;
  group: GroupProfile | null;
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "save", payload: { sessionId: string; name: string }): void;
}>();

const draftName = ref("");

watch(
  () => props.group?.name,
  (value) => {
    draftName.value = value ?? "";
  },
  { immediate: true },
);

const canSave = computed(() => {
  const nextName = draftName.value.trim();
  return !!props.session && !!nextName && nextName !== (props.group?.name ?? "");
});

function submit() {
  if (!props.session || !canSave.value) {
    return;
  }

  emit("save", {
    sessionId: props.session.id,
    name: draftName.value.trim(),
  });
}
</script>

<template>
  <OverlayPageShell
    title="Group Name"
    subtitle="Update the shared name shown in the session list and group header."
    @close="emit('close')"
  >
    <div v-if="session && group" class="group-name-body">
      <section class="editor-card">
        <label class="field-label" for="group-name-input">Display Name</label>
        <InputText
          id="group-name-input"
          v-model="draftName"
          placeholder="Launch Crew"
          class="name-input"
          @keyup.enter="submit"
        />
        <p class="field-hint">
          Changes apply locally to the group header, session title and member surfaces.
        </p>
      </section>
    </div>

    <div v-else class="missing-state">
      <i class="pi pi-users"></i>
      <p>This group is no longer available.</p>
    </div>

    <template v-if="session && group" #footer>
      <div class="footer-actions">
        <Button label="Cancel" text severity="secondary" @click="emit('close')" />
        <Button
          label="Save Group Name"
          icon="pi pi-check"
          severity="contrast"
          :disabled="!canSave"
          @click="submit"
        />
      </div>
    </template>
  </OverlayPageShell>
</template>

<style scoped>
.group-name-body,
.editor-card,
.footer-actions,
.missing-state {
  display: grid;
}

.group-name-body {
  gap: 18px;
}

.editor-card {
  gap: 12px;
  padding: 20px;
  border-radius: 24px;
  background:
    radial-gradient(circle at top right, rgba(106, 168, 255, 0.12), transparent 28%),
    #f7fafc;
}

.field-label {
  color: #6a7d98;
  text-transform: uppercase;
  letter-spacing: 0.14em;
  font-size: 0.72rem;
  font-weight: 700;
}

.name-input {
  width: 100%;
}

.field-hint,
.missing-state p {
  margin: 0;
  color: #71839d;
  line-height: 1.6;
}

.footer-actions {
  grid-auto-flow: column;
  justify-content: end;
  gap: 10px;
}

.missing-state {
  justify-items: center;
  align-content: center;
  min-height: 100%;
  gap: 10px;
  color: #6d809a;
}

.missing-state i {
  font-size: 2rem;
}

@media (max-width: 720px) {
  .footer-actions {
    grid-auto-flow: row;
  }
}
</style>
