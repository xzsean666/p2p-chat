<script setup lang="ts">
import { computed, ref, watch } from "vue";
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import Tag from "primevue/tag";
import OverlayPageShell from "./OverlayPageShell.vue";
import type { CircleItem, ContactItem, UpdateContactRemarkInput } from "../types/chat";

const props = defineProps<{
  contact: ContactItem | null;
  activeCircle: CircleItem | null;
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "open-join-circle"): void;
  (event: "toggle-block", contactId: string): void;
  (event: "send-message", contactId: string): void;
  (event: "save-remark", payload: UpdateContactRemarkInput): void;
}>();

const remarkDraft = ref("");

watch(
  () => props.contact,
  (contact) => {
    remarkDraft.value = contact?.subtitle ?? "";
  },
  { immediate: true },
);

const trimmedRemarkDraft = computed(() => remarkDraft.value.trim());
const remarkChanged = computed(() => {
  return trimmedRemarkDraft.value !== (props.contact?.subtitle ?? "");
});

function saveRemark() {
  if (!props.contact || !remarkChanged.value) {
    return;
  }

  emit("save-remark", {
    contactId: props.contact.id,
    remark: trimmedRemarkDraft.value,
  });
}
</script>

<template>
  <OverlayPageShell
    title="User Detail"
    subtitle="Profile, public key and quick actions."
    @close="emit('close')"
  >
    <div v-if="contact" class="profile-body">
      <section class="hero-card">
        <Avatar :label="contact.initials" shape="circle" class="hero-avatar" />
        <h2>{{ contact.name }}</h2>
        <p>{{ contact.handle }}</p>
        <Tag
          :value="contact.online ? 'Online' : 'Offline'"
          :severity="contact.online ? 'success' : 'secondary'"
          rounded
        />
      </section>

      <section class="section-card">
        <div class="section-title">Profile</div>
        <div class="info-list">
          <div class="info-row block">
            <span class="label">Remark</span>
            <InputText
              v-model="remarkDraft"
              placeholder="Add a local remark for this contact"
              @keydown.enter.prevent="saveRemark"
            />
            <p v-if="!trimmedRemarkDraft">No local remark saved yet.</p>
          </div>
          <div class="info-row block">
            <span class="label">Public Key</span>
            <code>{{ contact.pubkey }}</code>
          </div>
          <div class="info-row block">
            <span class="label">Bio</span>
            <p>{{ contact.bio }}</p>
          </div>
        </div>
      </section>
    </div>

    <div v-else class="missing-state">
      <i class="pi pi-user"></i>
      <p>This contact is no longer available.</p>
    </div>

    <template v-if="contact" #footer>
      <div class="profile-actions">
        <Button
          :icon="activeCircle ? 'pi pi-send' : 'pi pi-compass'"
          :label="activeCircle ? 'Send Message' : 'Join Circle'"
          severity="contrast"
          @click="activeCircle ? emit('send-message', contact.id) : emit('open-join-circle')"
        />
        <Button
          icon="pi pi-check"
          label="Save Remark"
          :disabled="!remarkChanged"
          severity="secondary"
          @click="saveRemark"
        />
        <Button
          :icon="contact.blocked ? 'pi pi-lock-open' : 'pi pi-ban'"
          :label="contact.blocked ? 'Unblock User' : 'Block User'"
          severity="danger"
          text
          @click="emit('toggle-block', contact.id)"
        />
      </div>
    </template>
  </OverlayPageShell>
</template>

<style scoped>
.profile-body,
.section-card,
.info-list {
  display: grid;
}

.profile-body {
  gap: 18px;
}

.hero-card {
  display: grid;
  justify-items: center;
  gap: 10px;
  padding: 28px 24px;
  border-radius: 28px;
  background:
    radial-gradient(circle at top left, rgba(106, 168, 255, 0.18), transparent 26%),
    linear-gradient(180deg, #f7fbfe 0%, #f2f7fb 100%);
  text-align: center;
}

.hero-avatar {
  width: 92px;
  height: 92px;
  background: linear-gradient(135deg, #dce9ff 0%, #d9f9ef 100%);
  color: #16355c;
  font-weight: 700;
  font-size: 1.45rem;
}

.hero-card h2,
.hero-card p,
.missing-state p,
.info-row p,
code {
  margin: 0;
}

.hero-card p {
  color: #6d809a;
}

.section-card {
  gap: 12px;
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
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 16px 18px;
  border-radius: 20px;
  background: #f7fafc;
}

.info-row.block {
  display: grid;
  justify-content: stretch;
}

.label {
  color: #70839d;
  font-size: 0.82rem;
  text-transform: uppercase;
  letter-spacing: 0.12em;
}

.info-row strong,
.info-row p,
code {
  color: #415772;
  line-height: 1.65;
}

.info-row :deep(.p-inputtext) {
  width: 100%;
}

code {
  font-family: "IBM Plex Mono", monospace;
  word-break: break-all;
}

.profile-actions {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
}

.missing-state {
  display: grid;
  justify-items: center;
  gap: 10px;
  min-height: 100%;
  align-content: center;
  color: #6d809a;
}

.missing-state i {
  font-size: 2rem;
}
</style>
