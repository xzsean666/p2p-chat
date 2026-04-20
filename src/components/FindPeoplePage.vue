<script setup lang="ts">
import { computed, ref } from "vue";
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import Tag from "primevue/tag";
import OverlayPageShell from "./OverlayPageShell.vue";
import { classifyChatQuery, isCircleQuery } from "../services/chatQueryIntents";
import type { CircleItem, ContactItem } from "../types/chat";

const props = defineProps<{
  contacts: ContactItem[];
  currentCircleContactIds: string[];
  circle: CircleItem | null;
  mode: "chat" | "join-circle";
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "open-contact", contactId: string): void;
  (event: "select-contact", contactId: string): void;
  (event: "lookup-contact", query: string): void;
  (event: "join-circle", query: string): void;
}>();

const keyword = ref("");
const pasteFeedback = ref<"" | "pasted" | "failed">("");

const currentCircleContactSet = computed(() => new Set(props.currentCircleContactIds));

const title = computed(() => {
  return props.mode === "join-circle" ? "Join Circle" : "Add Friends to Chat";
});

const subtitle = computed(() => {
  return props.mode === "join-circle"
    ? "Paste an invite link or relay endpoint to connect a new circle on desktop."
    : props.circle
      ? `Browse people around ${props.circle.name}`
      : "Browse the available contact list.";
});

const placeholder = computed(() => {
  return props.mode === "join-circle"
    ? "Paste an invite link or relay endpoint"
    : "Search by name, handle, pubkey or invite text";
});

const exactLocalMatch = computed(() => {
  if (props.mode !== "chat") {
    return null;
  }

  const normalized = keyword.value.trim().toLowerCase();
  if (!normalized) {
    return null;
  }

  return (
    props.contacts.find((contact) => {
      return [contact.id, contact.name, contact.handle, contact.pubkey]
        .map((value) => value.trim().toLowerCase())
        .includes(normalized);
    }) ?? null
  );
});

const filteredContacts = computed(() => {
  if (props.mode !== "chat") {
    return [];
  }

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

const queryType = computed(() => {
  const kind = classifyChatQuery(keyword.value);
  if (!kind) {
    return null;
  }

  switch (kind) {
    case "relay":
      return "Relay";
    case "invite":
      return "Invite";
    case "handle":
      return "Handle";
    case "pubkey":
      return "Pubkey";
    default:
      return props.mode === "join-circle" ? "Join Link" : "Lookup";
  }
});

const showLookupCard = computed(() => {
  const value = keyword.value.trim();
  if (!value) {
    return false;
  }

  if (props.mode === "join-circle") {
    return true;
  }

  return !exactLocalMatch.value;
});

const circleInputDetected = computed(() => {
  if (props.mode === "join-circle") {
    return true;
  }

  return isCircleQuery(keyword.value);
});

function contactMetaLine(contact: ContactItem) {
  return contact.subtitle ? `${contact.handle} · ${contact.subtitle}` : contact.handle;
}

function submitJoinCircle() {
  const value = keyword.value.trim();
  if (!value) {
    return;
  }

  emit("join-circle", value);
}

function submitContactLookup() {
  const value = keyword.value.trim();
  if (!value) {
    return;
  }

  emit("lookup-contact", value);
}

function submitLookup() {
  if (props.mode === "join-circle" || circleInputDetected.value) {
    submitJoinCircle();
    return;
  }

  submitContactLookup();
}

async function pasteFromClipboard() {
  try {
    const value = await navigator.clipboard.readText();
    if (!value.trim()) {
      pasteFeedback.value = "failed";
    } else {
      keyword.value = value.trim();
      pasteFeedback.value = "pasted";
    }
  } catch {
    pasteFeedback.value = "failed";
  }

  window.setTimeout(() => {
    pasteFeedback.value = "";
  }, 1800);
}
</script>

<template>
  <OverlayPageShell :title="title" :subtitle="subtitle" @close="emit('close')">
    <div class="find-page">
      <section class="search-card">
        <div class="search-field">
          <i class="pi pi-search"></i>
          <InputText
            v-model="keyword"
            :placeholder="placeholder"
            @keydown.enter.prevent="submitLookup"
          />
          <Button
            icon="pi pi-arrow-right"
            rounded
            text
            severity="secondary"
            :aria-label="mode === 'join-circle' || circleInputDetected ? 'Connect circle' : 'Run lookup'"
            @click="submitLookup"
          />
        </div>

        <div class="tool-row">
          <Button
            icon="pi pi-copy"
            label="Paste from Clipboard"
            text
            severity="secondary"
            @click="pasteFromClipboard"
          />
          <Tag :value="mode === 'join-circle' || circleInputDetected ? 'Circle Import' : 'Lookup'" severity="contrast" rounded />
        </div>
        <p v-if="pasteFeedback === 'pasted'" class="feedback success">Clipboard value pasted into the field.</p>
        <p v-else-if="pasteFeedback === 'failed'" class="feedback">Clipboard is unavailable or empty in this environment.</p>
      </section>

      <section v-if="showLookupCard" class="lookup-card">
        <div class="lookup-copy">
          <div class="lookup-head">
            <strong>{{ queryType }} {{ mode === "join-circle" ? "Connect" : "Lookup" }}</strong>
            <Tag :value="queryType || 'Input'" severity="contrast" rounded />
          </div>
          <p>{{ keyword.trim() }}</p>
          <span>
            {{
              mode === "join-circle" || circleInputDetected
                ? "This looks like a circle invite or relay address. The desktop flow will resolve it as a circle import by default."
                : "This will create a local contact shell when no matching person already exists."
            }}
          </span>
        </div>

        <div class="lookup-actions">
          <Button
            :icon="mode === 'join-circle' || circleInputDetected ? 'pi pi-compass' : 'pi pi-send'"
            :label="mode === 'join-circle' || circleInputDetected ? 'Join Circle' : 'Start from Lookup'"
            severity="contrast"
            @click="submitLookup"
          />
          <Button
            v-if="mode === 'chat' && circleInputDetected"
            icon="pi pi-user-plus"
            label="Treat as Person Lookup"
            text
            severity="secondary"
            @click="submitContactLookup"
          />
        </div>
      </section>

      <section v-if="mode === 'chat' && groupedContacts.length" class="grouped-list">
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
        </div>
      </section>

      <section v-else-if="mode === 'chat'" class="empty-state">
        <i class="pi pi-users"></i>
        <h3>No Results</h3>
        <p>Nothing matched your search. Try a shorter keyword or start directly from a handle, pubkey or invite-like text.</p>
      </section>

      <section v-else class="join-mode-card">
        <div class="join-copy">
          <h3>Join with invite or relay URL</h3>
          <p>Use invite links for imported circles, or paste a `wss://` endpoint for a manual relay entry.</p>
        </div>

        <div class="join-grid">
          <div class="join-item">
            <strong>Invite Links</strong>
            <p>`p2pchat://...`, `invite://...` and other invite-like text will be treated as join flow input.</p>
          </div>
          <div class="join-item">
            <strong>Relay Endpoints</strong>
            <p>`wss://relay.example.com` and compatible runtime URLs will be added as custom relay entries.</p>
          </div>
        </div>
      </section>
    </div>
  </OverlayPageShell>
</template>

<style scoped>
.find-page,
.search-card,
.grouped-list,
.letter-group,
.group-list,
.lookup-card,
.lookup-copy,
.join-mode-card,
.join-grid,
.join-item {
  display: grid;
}

.find-page {
  gap: 18px;
}

.search-card,
.lookup-card,
.join-mode-card {
  padding: 18px;
  border-radius: 24px;
}

.search-card,
.join-mode-card {
  background: #f8fbfd;
}

.lookup-card {
  gap: 14px;
  align-items: center;
  background:
    radial-gradient(circle at top left, rgba(106, 168, 255, 0.14), transparent 24%),
    linear-gradient(180deg, #f7fbfe 0%, #f2f7fb 100%);
}

.search-field,
.contact-row,
.contact-main,
.contact-head,
.contact-actions,
.lookup-head,
.tool-row,
.lookup-actions {
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

.tool-row {
  justify-content: space-between;
  gap: 10px;
  margin-top: 10px;
}

.lookup-actions {
  gap: 10px;
  flex-wrap: wrap;
}

.feedback,
.lookup-copy p,
.lookup-copy span,
.contact-copy p,
.empty-state p,
.join-copy p,
.join-item p {
  margin: 0;
  color: #6d809a;
}

.feedback.success {
  color: #2d7a53;
}

.lookup-copy {
  gap: 8px;
}

.lookup-head {
  justify-content: space-between;
  gap: 10px;
}

.lookup-copy strong,
.lookup-copy p,
.lookup-copy span,
.contact-copy strong,
.contact-copy p,
.empty-state h3,
.empty-state p,
.join-copy h3,
.join-copy p,
.join-item strong,
.join-item p {
  margin: 0;
}

.grouped-list,
.letter-group,
.group-list,
.join-grid {
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

.empty-state,
.join-item {
  padding: 18px;
  border-radius: 20px;
  background: #ffffff;
}

.empty-state {
  display: grid;
  justify-items: center;
  gap: 12px;
  text-align: center;
}

.empty-state i {
  font-size: 2rem;
  color: #7d8ea6;
}

.join-mode-card {
  gap: 14px;
}

.join-copy,
.join-item {
  gap: 6px;
}
</style>
