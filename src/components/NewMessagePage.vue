<script setup lang="ts">
import { computed, ref } from "vue";
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import Chip from "primevue/chip";
import InputText from "primevue/inputtext";
import Tag from "primevue/tag";
import OverlayPageShell from "./OverlayPageShell.vue";
import type { CircleItem, ContactItem } from "../types/chat";

const props = defineProps<{
  contacts: ContactItem[];
  currentCircleContactIds: string[];
  circle: CircleItem | null;
  inviteLink: string;
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "select-contact", contactId: string): void;
  (event: "open-find-people"): void;
}>();

const keyword = ref("");
const inviteStatus = ref<"idle" | "copied" | "failed">("idle");

const currentCircleContactSet = computed(() => new Set(props.currentCircleContactIds));

const filteredContacts = computed(() => {
  const value = keyword.value.trim().toLowerCase();
  if (!value) {
    return props.contacts;
  }

  return props.contacts.filter((contact) => {
    return [contact.name, contact.handle, contact.subtitle, contact.bio]
      .join(" ")
      .toLowerCase()
      .includes(value);
  });
});

const suggestedContacts = computed(() => {
  return filteredContacts.value.filter((contact) => {
    return !currentCircleContactSet.value.has(contact.id);
  });
});

const existingContacts = computed(() => {
  return filteredContacts.value.filter((contact) => {
    return currentCircleContactSet.value.has(contact.id);
  });
});

async function copyInviteLink() {
  try {
    await navigator.clipboard.writeText(props.inviteLink);
    inviteStatus.value = "copied";
  } catch {
    inviteStatus.value = "failed";
  }

  window.setTimeout(() => {
    inviteStatus.value = "idle";
  }, 1800);
}
</script>

<template>
  <OverlayPageShell
    title="New Message"
    :subtitle="circle ? `Start a chat in ${circle.name}` : 'Start a direct conversation.'"
    @close="emit('close')"
  >
    <div class="new-message-body">
      <section class="hero-card">
        <div class="hero-copy">
          <div class="hero-head">
            <h3>{{ circle?.name ?? "No Circle" }}</h3>
            <Tag
              v-if="circle"
              :value="circle.status"
              :severity="circle.status === 'open' ? 'success' : circle.status === 'connecting' ? 'warn' : 'secondary'"
              rounded
            />
          </div>
          <p>{{ circle?.description ?? "Choose a circle before you start new chats." }}</p>
        </div>

        <div class="hero-actions">
          <Button
            icon="pi pi-search"
            label="Find People"
            severity="contrast"
            @click="emit('open-find-people')"
          />
          <Button
            icon="pi pi-link"
            label="Copy Invite"
            text
            severity="contrast"
            @click="copyInviteLink"
          />
        </div>

        <p v-if="inviteStatus === 'copied'" class="invite-feedback success">
          Invite link copied to clipboard.
        </p>
        <p v-else-if="inviteStatus === 'failed'" class="invite-feedback">
          Clipboard is unavailable in this environment.
        </p>
      </section>

      <section class="search-card">
        <div class="search-field">
          <i class="pi pi-search"></i>
          <InputText v-model="keyword" placeholder="Search contacts, handles or bio" />
        </div>

        <div class="chip-row">
          <Chip :label="`${contacts.length} Contacts`" />
          <Chip :label="`${suggestedContacts.length} Suggested`" />
          <Chip :label="`${existingContacts.length} In Circle`" />
        </div>
      </section>

      <section v-if="suggestedContacts.length" class="list-section">
        <div class="section-title">Suggested</div>
        <div class="contact-list">
          <button
            v-for="contact in suggestedContacts"
            :key="contact.id"
            type="button"
            class="contact-row"
            @click="emit('select-contact', contact.id)"
          >
            <Avatar :label="contact.initials" shape="circle" class="contact-avatar" />
            <div class="contact-copy">
              <div class="contact-head">
                <strong>{{ contact.name }}</strong>
                <span v-if="contact.online" class="online-dot"></span>
              </div>
              <p>{{ contact.subtitle }}</p>
            </div>
            <Tag value="New" severity="contrast" rounded />
          </button>
        </div>
      </section>

      <section v-if="existingContacts.length" class="list-section">
        <div class="section-title">Current Circle</div>
        <div class="contact-list">
          <button
            v-for="contact in existingContacts"
            :key="contact.id"
            type="button"
            class="contact-row"
            @click="emit('select-contact', contact.id)"
          >
            <Avatar :label="contact.initials" shape="circle" class="contact-avatar" />
            <div class="contact-copy">
              <div class="contact-head">
                <strong>{{ contact.name }}</strong>
                <span v-if="contact.online" class="online-dot"></span>
              </div>
              <p>{{ contact.subtitle }}</p>
            </div>
            <Tag value="Existing" severity="secondary" rounded />
          </button>
        </div>
      </section>

      <section v-if="!filteredContacts.length" class="empty-state">
        <i class="pi pi-user-plus"></i>
        <h3>No Matching Contacts</h3>
        <p>Try a different keyword or open Find People to browse the full contact list.</p>
      </section>
    </div>
  </OverlayPageShell>
</template>

<style scoped>
.new-message-body,
.hero-card,
.hero-copy,
.search-card,
.contact-list {
  display: grid;
}

.new-message-body {
  gap: 18px;
}

.hero-card {
  gap: 14px;
  padding: 24px;
  border-radius: 28px;
  background:
    radial-gradient(circle at top left, rgba(106, 168, 255, 0.18), transparent 26%),
    linear-gradient(180deg, #f7fbfe 0%, #f2f7fb 100%);
}

.hero-copy {
  gap: 8px;
}

.hero-head,
.hero-actions,
.search-field,
.contact-row,
.contact-head,
.chip-row {
  display: flex;
  align-items: center;
}

.hero-head {
  justify-content: space-between;
  gap: 12px;
}

.hero-copy h3,
.hero-copy p,
.section-title,
.empty-state h3,
.empty-state p {
  margin: 0;
}

.hero-copy p {
  color: #6b7d97;
  line-height: 1.65;
}

.hero-actions {
  gap: 10px;
  flex-wrap: wrap;
}

.invite-feedback {
  color: #7b8ca5;
  font-size: 0.9rem;
}

.invite-feedback.success {
  color: #2f8c6a;
}

.search-card {
  gap: 12px;
  padding: 18px;
  border-radius: 24px;
  background: #f8fbfd;
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

.chip-row {
  gap: 8px;
  flex-wrap: wrap;
}

.list-section {
  display: grid;
  gap: 10px;
}

.section-title {
  color: #6a7d98;
  text-transform: uppercase;
  letter-spacing: 0.14em;
  font-size: 0.72rem;
}

.contact-list {
  gap: 10px;
}

.contact-row {
  gap: 12px;
  width: 100%;
  padding: 14px 12px;
  border: 0;
  border-radius: 20px;
  background: #f7fafc;
  text-align: left;
  cursor: pointer;
}

.contact-row:hover {
  background: #f2f7fb;
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
  flex: 1;
}

.contact-head {
  gap: 8px;
}

.contact-copy strong,
.contact-copy p {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin: 0;
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
</style>
