<script setup lang="ts">
import { computed, ref } from "vue";
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import Chip from "primevue/chip";
import InputText from "primevue/inputtext";
import Tag from "primevue/tag";
import OverlayPageShell from "./OverlayPageShell.vue";
import SelfChatIcon from "./SelfChatIcon.vue";
import type { CircleItem, ContactItem } from "../types/chat";

const props = defineProps<{
  contacts: ContactItem[];
  currentCircleContactIds: string[];
  circle: CircleItem | null;
  inviteLink: string;
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "open-contact", contactId: string): void;
  (event: "select-contact", contactId: string): void;
  (event: "open-self-confirm"): void;
  (event: "open-group-select"): void;
  (event: "open-find-people"): void;
}>();

const keyword = ref("");
const inviteStatus = ref<"idle" | "copied" | "failed">("idle");

const currentCircleContactSet = computed(() => new Set(props.currentCircleContactIds));

const filteredContacts = computed(() => {
  const value = keyword.value.trim().toLowerCase();
  return props.contacts
    .filter((contact) => {
      if (!value) {
        return true;
      }

      return [contact.name, contact.handle, contact.subtitle, contact.bio, contact.pubkey]
        .join(" ")
        .toLowerCase()
        .includes(value);
    })
    .sort((left, right) => left.name.localeCompare(right.name));
});

const highlightedContacts = computed(() => filteredContacts.value.slice(0, 10));

function contactMetaLine(contact: ContactItem) {
  return contact.subtitle ? `${contact.handle} · ${contact.subtitle}` : contact.handle;
}

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
    :subtitle="circle ? `Start a chat inside ${circle.name}` : 'Choose how you want to begin a conversation.'"
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

        <div class="quick-grid">
          <button type="button" class="quick-card" @click="emit('open-self-confirm')">
            <SelfChatIcon size="md" />
            <div class="quick-copy">
              <strong>Note to Self</strong>
              <p>Open the private note space for this circle.</p>
            </div>
          </button>

          <button type="button" class="quick-card" @click="emit('open-group-select')">
            <i class="pi pi-users quick-icon"></i>
            <div class="quick-copy">
              <strong>New Group</strong>
              <p>Select members first, then confirm the final group setup.</p>
            </div>
          </button>

          <button type="button" class="quick-card" @click="emit('open-find-people')">
            <i class="pi pi-search quick-icon"></i>
            <div class="quick-copy">
              <strong>Add Friends</strong>
              <p>Look up a handle, pubkey or invite-style text before chatting.</p>
            </div>
          </button>

          <button type="button" class="quick-card" @click="copyInviteLink">
            <i class="pi pi-link quick-icon"></i>
            <div class="quick-copy">
              <strong>Invite Friends</strong>
              <p>Copy the current circle invite and share it from the desktop shell.</p>
            </div>
          </button>
        </div>

        <div class="chip-row">
          <Chip :label="`${contacts.length} Contacts`" />
          <Chip :label="`${currentCircleContactIds.length} Already in Circle`" />
        </div>

        <p v-if="inviteStatus === 'copied'" class="invite-feedback success">
          Invite link copied to clipboard.
        </p>
        <p v-else-if="inviteStatus === 'failed'" class="invite-feedback">
          Clipboard is unavailable in this environment.
        </p>
      </section>

      <section class="search-card">
        <div class="section-head">
          <div>
            <div class="section-title">Start With A Person</div>
            <p>Tap a profile to inspect it first, or use the send action for a direct jump into chat.</p>
          </div>
        </div>

        <div class="search-field">
          <i class="pi pi-search"></i>
          <InputText v-model="keyword" placeholder="Search contacts, handles or pubkeys" />
        </div>
      </section>

      <section v-if="highlightedContacts.length" class="list-section">
        <div class="contact-list">
          <div v-for="contact in highlightedContacts" :key="contact.id" class="contact-row">
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
      </section>

      <section v-else class="empty-state">
        <i class="pi pi-user-plus"></i>
        <h3>No Matching Contacts</h3>
        <p>Try a different keyword or switch to Add Friends for handle and pubkey lookup.</p>
      </section>
    </div>
  </OverlayPageShell>
</template>

<style scoped>
.new-message-body,
.hero-card,
.hero-copy,
.search-card,
.contact-list,
.quick-grid {
  display: grid;
}

.new-message-body {
  gap: 18px;
}

.hero-card,
.search-card {
  gap: 14px;
  padding: 24px;
  border-radius: 28px;
}

.hero-card {
  background:
    radial-gradient(circle at top left, rgba(106, 168, 255, 0.18), transparent 26%),
    linear-gradient(180deg, #f7fbfe 0%, #f2f7fb 100%);
}

.search-card {
  background: #f8fbfd;
}

.hero-copy {
  gap: 8px;
}

.hero-head,
.search-field,
.contact-head,
.chip-row,
.section-head,
.contact-actions,
.contact-row,
.contact-main {
  display: flex;
  align-items: center;
}

.hero-head,
.section-head,
.contact-row {
  justify-content: space-between;
  gap: 12px;
}

.hero-card h3,
.hero-card p,
.quick-copy strong,
.quick-copy p,
.section-head p,
.contact-copy strong,
.contact-copy p,
.invite-feedback,
.empty-state h3,
.empty-state p {
  margin: 0;
}

.hero-card p,
.quick-copy p,
.section-head p,
.contact-copy p,
.invite-feedback,
.empty-state p {
  color: #6d809a;
}

.quick-grid {
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: 12px;
}

.quick-card {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 16px;
  border: 0;
  border-radius: 22px;
  background: rgba(255, 255, 255, 0.82);
  text-align: left;
  cursor: pointer;
}

.quick-card:hover {
  background: rgba(255, 255, 255, 0.96);
}

.quick-icon {
  font-size: 1.1rem;
  color: #44648c;
  margin-top: 4px;
}

.quick-copy {
  display: grid;
  gap: 4px;
}

.chip-row {
  gap: 8px;
  flex-wrap: wrap;
}

.invite-feedback.success {
  color: #2d7a53;
}

.section-title {
  color: #6a7d98;
  text-transform: uppercase;
  letter-spacing: 0.14em;
  font-size: 0.72rem;
}

.search-field {
  gap: 10px;
  padding: 12px 14px;
  border: 1px solid #d8e2ef;
  border-radius: 18px;
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

.list-section,
.contact-list {
  display: grid;
  gap: 10px;
}

.contact-row {
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
.contact-copy p {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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

.empty-state {
  display: grid;
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
</style>
