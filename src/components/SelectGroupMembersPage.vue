<script setup lang="ts">
import { computed, ref } from "vue";
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import Tag from "primevue/tag";
import OverlayPageShell from "./OverlayPageShell.vue";
import type { ContactItem } from "../types/chat";

const props = defineProps<{
  contacts: ContactItem[];
  currentCircleContactIds: string[];
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "open-contact", contactId: string): void;
  (event: "next", memberContactIds: string[]): void;
}>();

const keyword = ref("");
const selectedContactIds = ref<string[]>([]);

const selectedContactSet = computed(() => new Set(selectedContactIds.value));
const currentCircleContactSet = computed(() => new Set(props.currentCircleContactIds));

const filteredContacts = computed(() => {
  const value = keyword.value.trim().toLowerCase();
  return props.contacts
    .filter((contact) => {
      if (!value) {
        return true;
      }

      return [contact.name, contact.handle, contact.subtitle, contact.bio, contact.pubkey, contact.ethereumAddress]
        .join(" ")
        .toLowerCase()
        .includes(value);
    })
    .sort((left, right) => left.name.localeCompare(right.name));
});

function contactMetaLine(contact: ContactItem) {
  return contact.subtitle ? `${contact.handle} · ${contact.subtitle}` : contact.handle;
}

function toggleContact(contactId: string) {
  if (selectedContactSet.value.has(contactId)) {
    selectedContactIds.value = selectedContactIds.value.filter((id) => id !== contactId);
    return;
  }

  selectedContactIds.value = [...selectedContactIds.value, contactId];
}

function submit() {
  if (!selectedContactIds.value.length) {
    return;
  }

  emit("next", selectedContactIds.value);
}
</script>

<template>
  <OverlayPageShell
    title="New Group"
    subtitle="Select the people you want to include before confirming the final group setup."
    @close="emit('close')"
  >
    <div class="members-body">
      <section class="search-card">
        <div class="search-field">
          <i class="pi pi-search"></i>
          <InputText v-model="keyword" placeholder="Search contacts for the new group" />
        </div>
        <div class="stats-row">
          <Tag :value="`${contacts.length} Contacts`" severity="secondary" rounded />
          <Tag :value="`${selectedContactIds.length} Selected`" severity="contrast" rounded />
        </div>
      </section>

      <section v-if="filteredContacts.length" class="contact-list">
        <div v-for="contact in filteredContacts" :key="contact.id" class="contact-row">
          <button type="button" class="contact-main" @click="emit('open-contact', contact.id)">
            <Avatar :label="contact.initials" shape="circle" class="contact-avatar" />
            <div class="contact-copy">
              <div class="contact-head">
                <strong>{{ contact.name }}</strong>
                <span v-if="contact.online" class="online-dot"></span>
              </div>
              <p>{{ contactMetaLine(contact) }}</p>
            </div>
          </button>

          <div class="contact-actions">
            <Tag
              :value="currentCircleContactSet.has(contact.id) ? 'In Circle' : 'Outside Circle'"
              :severity="currentCircleContactSet.has(contact.id) ? 'secondary' : 'contrast'"
              rounded
            />
            <Button
              :icon="selectedContactSet.has(contact.id) ? 'pi pi-check-circle' : 'pi pi-plus-circle'"
              rounded
              text
              severity="secondary"
              :aria-label="selectedContactSet.has(contact.id) ? 'Remove member' : 'Add member'"
              @click="toggleContact(contact.id)"
            />
          </div>
        </div>
      </section>

      <section v-else class="empty-state">
        <i class="pi pi-users"></i>
        <h3>No Contacts Found</h3>
        <p>Try a different keyword before moving on to group creation.</p>
      </section>
    </div>

    <template #footer>
      <div class="footer-actions">
        <Button label="Cancel" text severity="secondary" @click="emit('close')" />
        <Button
          label="Next"
          icon="pi pi-arrow-right"
          severity="contrast"
          :disabled="!selectedContactIds.length"
          @click="submit"
        />
      </div>
    </template>
  </OverlayPageShell>
</template>

<style scoped>
.members-body,
.search-card,
.contact-list,
.footer-actions,
.empty-state {
  display: grid;
}

.members-body {
  gap: 18px;
}

.search-card {
  gap: 12px;
  padding: 20px;
  border-radius: 24px;
  background: #f8fbfd;
}

.search-field,
.stats-row,
.contact-row,
.contact-main,
.contact-head,
.contact-actions {
  display: flex;
  align-items: center;
}

.search-field {
  gap: 10px;
  padding: 12px 14px;
  border-radius: 18px;
  background: #ffffff;
  border: 1px solid #d8e2ef;
}

.search-field i {
  color: #7b8ca5;
}

.search-field :deep(.p-inputtext) {
  width: 100%;
  border: 0;
  box-shadow: none;
  background: transparent;
  padding-left: 0;
}

.stats-row {
  gap: 8px;
  flex-wrap: wrap;
}

.contact-list {
  gap: 10px;
}

.contact-row {
  justify-content: space-between;
  gap: 12px;
  padding: 12px;
  border-radius: 20px;
  background: #f7fafc;
}

.contact-main {
  flex: 1;
  gap: 12px;
  padding: 0;
  border: 0;
  background: transparent;
  text-align: left;
  cursor: pointer;
}

.contact-avatar {
  width: 42px;
  height: 42px;
  background: linear-gradient(135deg, #dce9ff 0%, #d9f9ef 100%);
  color: #16355c;
  font-weight: 700;
}

.contact-copy {
  min-width: 0;
}

.contact-copy strong,
.contact-copy p,
.empty-state h3,
.empty-state p {
  margin: 0;
}

.contact-copy strong,
.contact-copy p {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.contact-copy p,
.empty-state p {
  color: #6d809a;
}

.contact-head {
  gap: 8px;
}

.online-dot {
  width: 8px;
  height: 8px;
  border-radius: 999px;
  background: #35c98d;
}

.contact-actions {
  gap: 8px;
}

.footer-actions {
  grid-auto-flow: column;
  justify-content: end;
  gap: 10px;
}

.empty-state {
  justify-items: center;
  gap: 12px;
  padding: 40px 24px;
  border-radius: 28px;
  background: #f8fbfd;
  text-align: center;
}

.empty-state i {
  font-size: 2rem;
  color: #7d8ea6;
}

@media (max-width: 720px) {
  .footer-actions {
    grid-auto-flow: row;
  }
}
</style>
