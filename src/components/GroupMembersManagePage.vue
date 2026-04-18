<script setup lang="ts">
import { computed, ref, watch } from "vue";
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import Tag from "primevue/tag";
import OverlayPageShell from "./OverlayPageShell.vue";
import type { ContactItem, GroupProfile, SessionItem } from "../types/chat";

const props = defineProps<{
  session: SessionItem | null;
  group: GroupProfile | null;
  memberContacts: ContactItem[];
  candidateContacts: ContactItem[];
  mode: "add" | "remove";
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "save", payload: { sessionId: string; memberContactIds: string[] }): void;
  (event: "open-member", contactId: string): void;
}>();

const keyword = ref("");
const selectedContactIds = ref<string[]>([]);

watch(
  () => [props.mode, props.session?.id, props.memberContacts.length, props.candidateContacts.length].join(":"),
  () => {
    selectedContactIds.value = [];
    keyword.value = "";
  },
  { immediate: true },
);

const sourceContacts = computed(() => {
  return props.mode === "add" ? props.candidateContacts : props.memberContacts;
});

const filteredContacts = computed(() => {
  const value = keyword.value.trim().toLowerCase();
  if (!value) {
    return sourceContacts.value;
  }

  return sourceContacts.value.filter((contact) => {
    return [contact.name, contact.handle, contact.subtitle, contact.bio]
      .join(" ")
      .toLowerCase()
      .includes(value);
  });
});

const selectedContactSet = computed(() => new Set(selectedContactIds.value));
const canSave = computed(() => {
  if (props.mode === "add") {
    return selectedContactIds.value.length > 0;
  }

  return (
    selectedContactIds.value.length > 0 &&
    props.memberContacts.length - selectedContactIds.value.length > 0
  );
});

const saveLabel = computed(() => {
  return props.mode === "add" ? "Add Members" : "Remove Members";
});

const title = computed(() => {
  return props.mode === "add" ? "Add Members" : "Remove Members";
});

const subtitle = computed(() => {
  return props.mode === "add"
    ? "Select contacts to append to the current member roster."
    : "Pick members to remove from this group.";
});

function toggleSelection(contactId: string) {
  if (selectedContactSet.value.has(contactId)) {
    selectedContactIds.value = selectedContactIds.value.filter((id) => id !== contactId);
    return;
  }

  selectedContactIds.value = [...selectedContactIds.value, contactId];
}

function submit() {
  if (!props.session || !canSave.value) {
    return;
  }

  const memberContactIds =
    props.mode === "add"
      ? [
          ...props.memberContacts.map((contact) => contact.id),
          ...selectedContactIds.value,
        ]
      : props.memberContacts
          .map((contact) => contact.id)
          .filter((contactId) => !selectedContactSet.value.has(contactId));

  emit("save", {
    sessionId: props.session.id,
    memberContactIds,
  });
}
</script>

<template>
  <OverlayPageShell :title="title" :subtitle="subtitle" @close="emit('close')">
    <div v-if="session && group" class="members-manage-body">
      <section class="search-card">
        <div class="search-field">
          <i class="pi pi-search"></i>
          <InputText
            v-model="keyword"
            :placeholder="mode === 'add' ? 'Search contacts to add' : 'Search members to remove'"
          />
        </div>

        <div class="stats-row">
          <Tag :value="`${sourceContacts.length} candidates`" severity="secondary" rounded />
          <Tag :value="`${selectedContactIds.length} selected`" severity="contrast" rounded />
        </div>
      </section>

      <section class="list-card">
        <div v-if="!filteredContacts.length" class="empty-state">
          <i class="pi pi-users"></i>
          <p>{{ mode === "add" ? "No additional contacts are available." : "No removable members matched." }}</p>
        </div>

        <div v-else class="member-list">
          <div
            v-for="contact in filteredContacts"
            :key="contact.id"
            class="member-row"
          >
            <button type="button" class="member-main" @click="emit('open-member', contact.id)">
              <Avatar :label="contact.initials" shape="circle" class="member-avatar" />
              <div class="member-copy">
                <strong>{{ contact.name }}</strong>
                <span>{{ contact.handle }}</span>
              </div>
            </button>

            <Button
              :icon="selectedContactSet.has(contact.id) ? 'pi pi-check-circle' : mode === 'add' ? 'pi pi-plus-circle' : 'pi pi-minus-circle'"
              rounded
              text
              severity="secondary"
              :aria-label="selectedContactSet.has(contact.id) ? 'Undo selection' : saveLabel"
              @click="toggleSelection(contact.id)"
            />
          </div>
        </div>
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
          :label="saveLabel"
          :icon="mode === 'add' ? 'pi pi-user-plus' : 'pi pi-user-minus'"
          severity="contrast"
          :disabled="!canSave"
          @click="submit"
        />
      </div>
    </template>
  </OverlayPageShell>
</template>

<style scoped>
.members-manage-body,
.search-card,
.stats-row,
.list-card,
.member-list,
.member-copy,
.footer-actions,
.missing-state,
.empty-state {
  display: grid;
}

.members-manage-body {
  gap: 18px;
}

.search-card {
  gap: 12px;
  padding: 18px;
  border-radius: 24px;
  background: #f7fafc;
}

.search-field {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 12px 14px;
  border-radius: 18px;
  background: rgba(255, 255, 255, 0.92);
}

.search-field i {
  color: #7d8da5;
}

.search-field :deep(.p-inputtext) {
  width: 100%;
  border: 0;
  box-shadow: none;
  background: transparent;
  padding: 0;
}

.stats-row {
  grid-auto-flow: column;
  justify-content: start;
  gap: 8px;
}

.list-card {
  gap: 10px;
}

.member-list {
  gap: 10px;
}

.member-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 12px;
  border-radius: 20px;
  background: #f7fafc;
}

.member-main {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
  flex: 1;
  border: 0;
  background: transparent;
  padding: 0;
  text-align: left;
  cursor: pointer;
}

.member-avatar {
  width: 42px;
  height: 42px;
  background: linear-gradient(135deg, #dce9ff 0%, #d9f9ef 100%);
  color: #16355c;
  font-weight: 700;
}

.member-copy {
  gap: 4px;
  min-width: 0;
}

.member-copy strong,
.member-copy span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.member-copy span {
  color: #71839d;
}

.footer-actions {
  grid-auto-flow: column;
  justify-content: end;
  gap: 10px;
}

.missing-state,
.empty-state {
  justify-items: center;
  align-content: center;
  gap: 10px;
  color: #6d809a;
}

.empty-state {
  min-height: 220px;
  border-radius: 24px;
  background: #f7fafc;
}

.missing-state {
  min-height: 100%;
}

.missing-state p,
.empty-state p {
  margin: 0;
}

.missing-state i,
.empty-state i {
  font-size: 2rem;
}

@media (max-width: 720px) {
  .footer-actions {
    grid-auto-flow: row;
  }
}
</style>
