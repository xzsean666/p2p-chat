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
const remarkChanged = computed(() => trimmedRemarkDraft.value !== (props.contact?.subtitle ?? ""));

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
  <OverlayPageShell title="Contact Info" subtitle="Profile" @close="emit('close')">
    <div v-if="contact" class="profile-page">
      <section class="profile-header">
        <Avatar :label="contact.initials" shape="circle" class="profile-avatar" />
        <h2>{{ contact.name }}</h2>
        <p>{{ contact.handle }}</p>
        <div class="profile-tags">
          <Tag :value="contact.online ? 'Online' : 'Offline'" :severity="contact.online ? 'success' : 'secondary'" rounded />
          <Tag v-if="contact.blocked" value="Blocked" severity="danger" rounded />
        </div>
      </section>

      <section class="info-section">
        <div class="section-title">Contact</div>
        <div class="section-list">
          <div class="field-row">
            <span class="label">Name</span>
            <strong>{{ contact.name }}</strong>
          </div>
          <div class="field-row">
            <span class="label">Status</span>
            <span>{{ contact.online ? "Online" : "Offline" }}</span>
          </div>
          <div class="field-row field-stack">
            <span class="label">Public Key</span>
            <code>{{ contact.pubkey }}</code>
          </div>
          <div v-if="contact.ethereumAddress" class="field-row field-stack">
            <span class="label">Ethereum Address</span>
            <code>{{ contact.ethereumAddress }}</code>
          </div>
          <div class="field-row field-stack">
            <span class="label">Bio</span>
            <p>{{ contact.bio || "No bio available." }}</p>
          </div>
        </div>
      </section>

      <section class="info-section">
        <div class="section-title">Local Note</div>
        <div class="section-list">
          <div class="field-row field-stack">
            <span class="label">Remark</span>
            <div class="remark-editor">
              <InputText
                v-model="remarkDraft"
                placeholder="Add a local remark for this contact"
                @keydown.enter.prevent="saveRemark"
              />
              <Button
                label="Save"
                :disabled="!remarkChanged"
                severity="secondary"
                text
                @click="saveRemark"
              />
            </div>
            <p v-if="!trimmedRemarkDraft">No local remark saved yet.</p>
          </div>
        </div>
      </section>

      <section class="info-section">
        <div class="section-title">Actions</div>
        <div class="section-list action-list">
          <Button
            :icon="activeCircle ? 'pi pi-send' : 'pi pi-compass'"
            :label="activeCircle ? 'Send Message' : 'Join Circle'"
            severity="contrast"
            text
            class="action-button"
            @click="activeCircle ? emit('send-message', contact.id) : emit('open-join-circle')"
          />
          <Button
            :icon="contact.blocked ? 'pi pi-lock-open' : 'pi pi-ban'"
            :label="contact.blocked ? 'Unblock User' : 'Block User'"
            severity="danger"
            text
            class="action-button"
            @click="emit('toggle-block', contact.id)"
          />
        </div>
      </section>
    </div>

    <div v-else class="missing-state">
      <i class="pi pi-user"></i>
      <p>This contact is no longer available.</p>
    </div>
  </OverlayPageShell>
</template>

<style scoped>
.profile-page,
.profile-header,
.info-section,
.section-list {
  display: grid;
}

.profile-page {
  gap: 22px;
  padding-top: 2px;
}

.profile-header {
  justify-items: center;
  gap: 8px;
  padding: 8px 0 2px;
  text-align: center;
}

.profile-avatar {
  width: 76px;
  height: 76px;
  background: #edf2f7;
  color: #2b4968;
  font-size: 1.2rem;
  font-weight: 700;
}

.profile-header h2,
.profile-header p,
.missing-state p,
.field-row p,
code {
  margin: 0;
}

.profile-header h2 {
  font-size: 1.28rem;
  font-weight: 600;
  color: #23364a;
}

.profile-header p {
  max-width: 32rem;
  color: #6d809a;
  line-height: 1.6;
}

.profile-tags {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 8px;
  margin-top: 4px;
}

.info-section {
  gap: 10px;
}

.section-title {
  color: #6a7d98;
  text-transform: uppercase;
  letter-spacing: 0.14em;
  font-size: 0.72rem;
}

.section-list {
  gap: 0;
  border: 1px solid #e7ebf1;
  border-radius: 18px;
  background: #ffffff;
  overflow: hidden;
}

.field-row,
.action-button {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 15px 16px;
  border-bottom: 1px solid #e7ebf1;
}

.field-row:last-child,
.action-list :deep(.p-button:last-child) {
  border-bottom: 0;
}

.field-stack {
  display: grid;
  justify-content: stretch;
}

.remark-editor {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 10px;
  align-items: center;
}

.label {
  color: #70839d;
  font-size: 0.82rem;
  text-transform: uppercase;
  letter-spacing: 0.12em;
}

.field-row strong,
.field-row span,
.field-row p,
code {
  color: #415772;
  line-height: 1.65;
}

.field-row :deep(.p-inputtext) {
  width: 100%;
}

.action-list :deep(.p-button) {
  justify-content: flex-start;
  width: 100%;
  padding: 15px 16px;
  border-bottom: 1px solid #e7ebf1;
  border-radius: 0;
}

code {
  font-family: "IBM Plex Mono", monospace;
  word-break: break-all;
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

@media (max-width: 720px) {
  .remark-editor {
    grid-template-columns: 1fr;
  }
}
</style>
