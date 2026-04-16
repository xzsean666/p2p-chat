<script setup lang="ts">
import { computed, ref } from "vue";
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import Tag from "primevue/tag";
import OverlayPageShell from "./OverlayPageShell.vue";
import type { CircleItem, ContactItem } from "../types/chat";

const props = defineProps<{
  contacts: ContactItem[];
  currentCircleContactIds: string[];
  circle: CircleItem | null;
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "open-contact", contactId: string): void;
  (event: "select-contact", contactId: string): void;
}>();

const keyword = ref("");

const currentCircleContactSet = computed(() => new Set(props.currentCircleContactIds));

const filteredContacts = computed(() => {
  const value = keyword.value.trim().toLowerCase();
  return props.contacts
    .filter((contact) => {
      if (!value) {
        return true;
      }

      return [contact.name, contact.handle, contact.subtitle, contact.bio]
        .join(" ")
        .toLowerCase()
        .includes(value);
    })
    .sort((left, right) => left.name.localeCompare(right.name));
});

const groupedContacts = computed(() => {
  const groups = new Map<string, ContactItem[]>();

  filteredContacts.value.forEach((contact) => {
    const key = contact.name.charAt(0).toUpperCase() || "#";
    if (!groups.has(key)) {
      groups.set(key, []);
    }

    groups.get(key)?.push(contact);
  });

  return Array.from(groups.entries()).map(([letter, items]) => ({ letter, items }));
});
</script>

<template>
  <OverlayPageShell
    title="Find People"
    :subtitle="circle ? `Browse people around ${circle.name}` : 'Browse the available contact list.'"
    @close="emit('close')"
  >
    <div class="find-page">
      <section class="search-card">
        <div class="search-field">
          <i class="pi pi-search"></i>
          <InputText v-model="keyword" placeholder="Search by name, handle or bio" />
        </div>
      </section>

      <section v-if="groupedContacts.length" class="grouped-list">
        <div v-for="group in groupedContacts" :key="group.letter" class="letter-group">
          <div class="group-letter">{{ group.letter }}</div>

          <div class="group-list">
            <div v-for="contact in group.items" :key="contact.id" class="contact-row">
              <button type="button" class="contact-main" @click="emit('open-contact', contact.id)">
                <Avatar :label="contact.initials" shape="circle" class="contact-avatar" />
                <div class="contact-copy">
                  <div class="contact-head">
                    <strong>{{ contact.name }}</strong>
                    <span v-if="contact.online" class="online-dot"></span>
                  </div>
                  <p>{{ contact.handle }} · {{ contact.subtitle }}</p>
                </div>
              </button>

              <div class="contact-actions">
                <Tag
                  :value="currentCircleContactSet.has(contact.id) ? 'In Circle' : 'Available'"
                  :severity="currentCircleContactSet.has(contact.id) ? 'secondary' : 'contrast'"
                  rounded
                />
                <Button
                  icon="pi pi-send"
                  rounded
                  severity="contrast"
                  aria-label="Start conversation"
                  @click="emit('select-contact', contact.id)"
                />
              </div>
            </div>
          </div>
        </div>
      </section>

      <section v-else class="empty-state">
        <i class="pi pi-users"></i>
        <h3>No Results</h3>
        <p>Nothing matched your search. Try a shorter keyword or browse from the main new-message page.</p>
      </section>
    </div>
  </OverlayPageShell>
</template>

<style scoped>
.find-page,
.search-card,
.grouped-list,
.letter-group,
.group-list {
  display: grid;
}

.find-page {
  gap: 18px;
}

.search-card {
  padding: 18px;
  border-radius: 24px;
  background: #f8fbfd;
}

.search-field,
.contact-row,
.contact-main,
.contact-head,
.contact-actions {
  display: flex;
  align-items: center;
}

.search-field {
  gap: 10px;
  padding: 0 14px;
  border: 1px solid #d8e2ef;
  border-radius: 16px;
  background: #ffffff;
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

.grouped-list,
.letter-group,
.group-list {
  gap: 10px;
}

.group-letter {
  color: #6a7d98;
  text-transform: uppercase;
  letter-spacing: 0.16em;
  font-size: 0.72rem;
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
  display: grid;
  gap: 4px;
  min-width: 0;
}

.contact-head {
  gap: 8px;
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

.contact-copy p {
  color: #6d809a;
}

.online-dot {
  width: 8px;
  height: 8px;
  border-radius: 999px;
  background: #38c389;
}

.contact-actions {
  gap: 8px;
  flex: none;
}

.empty-state {
  display: grid;
  justify-items: center;
  gap: 12px;
  padding: 42px 24px;
  border-radius: 28px;
  background: #f8fbfd;
  text-align: center;
}

.empty-state i {
  font-size: 2rem;
  color: #6d86a7;
}

.empty-state p {
  max-width: 42ch;
  color: #6d809a;
  line-height: 1.65;
}

@media (max-width: 720px) {
  .contact-row {
    align-items: flex-start;
    flex-direction: column;
  }

  .contact-actions {
    width: 100%;
    justify-content: space-between;
  }
}
</style>
