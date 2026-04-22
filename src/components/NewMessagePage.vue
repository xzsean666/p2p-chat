<script setup lang="ts">
import { computed, ref } from "vue";
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import OverlayPageShell from "./OverlayPageShell.vue";
import SelfChatIcon from "./SelfChatIcon.vue";
import type { CircleItem, ContactItem } from "../types/chat";

const props = defineProps<{
  contacts: ContactItem[];
  currentCircleContactIds: string[];
  circle: CircleItem | null;
  inviteLink: string;
  canInvite?: boolean;
  canAddFriends?: boolean;
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "open-contact", contactId: string): void;
  (event: "select-contact", contactId: string): void;
  (event: "open-self-confirm"): void;
  (event: "open-group-select"): void;
  (event: "open-find-people"): void;
  (event: "open-invite"): void;
}>();

type MessageListEntry =
  | {
      id: "self";
      kind: "self";
      name: string;
      meta: string;
      searchText: string;
      sortText: string;
    }
  | {
      id: string;
      kind: "contact";
      name: string;
      meta: string;
      searchText: string;
      sortText: string;
      contact: ContactItem;
    };

interface MessageListGroup {
  key: string;
  label: string;
  entries: MessageListEntry[];
}

const keyword = ref("");
const searchFocused = ref(false);

const currentCircleContactSet = computed(() => new Set(props.currentCircleContactIds));
const hasKeyword = computed(() => keyword.value.trim().length > 0);
const normalizedKeyword = computed(() => keyword.value.trim().toLowerCase());
const isSearchMode = computed(() => searchFocused.value || hasKeyword.value);

const selfEntry = computed<MessageListEntry>(() => {
  const meta = props.circle
    ? `Open your private memo thread for ${props.circle.name}.`
    : "Open your private memo thread.";

  return {
    id: "self",
    kind: "self",
    name: "Note to Self",
    meta,
    sortText: "Note to Self",
    searchText: [
      "note to self",
      "self",
      "memo",
      "private notes",
      props.circle?.name ?? "",
      props.circle?.description ?? "",
      meta,
    ]
      .join(" ")
      .toLowerCase(),
  };
});

const contactEntries = computed<MessageListEntry[]>(() => {
  return props.contacts.map((contact) => {
    const name = contact.name.trim() || contact.handle.trim() || contact.pubkey;
    const meta = contactMetaLine(contact);

    return {
      id: contact.id,
      kind: "contact",
      name,
      meta,
      sortText: name,
      searchText: [contact.name, contact.handle, contact.subtitle, contact.bio, contact.pubkey]
        .join(" ")
        .toLowerCase(),
      contact,
    };
  });
});

const allEntries = computed(() => {
  return [selfEntry.value, ...contactEntries.value].sort(compareEntries);
});

const searchResults = computed(() => {
  const value = normalizedKeyword.value;
  if (!value) {
    return [] as MessageListEntry[];
  }

  return allEntries.value.filter((entry) => entry.searchText.includes(value));
});

const groupedEntries = computed<MessageListGroup[]>(() => {
  const groups = new Map<string, MessageListEntry[]>();

  for (const entry of allEntries.value) {
    const key = groupKeyForEntry(entry.sortText);
    const bucket = groups.get(key);
    if (bucket) {
      bucket.push(entry);
      continue;
    }

    groups.set(key, [entry]);
  }

  return Array.from(groups.entries())
    .sort(([left], [right]) => compareGroupKeys(left, right))
    .map(([key, entries]) => ({
      key,
      label: key,
      entries,
    }));
});

const visibleGroups = computed<MessageListGroup[]>(() => {
  if (!isSearchMode.value) {
    return groupedEntries.value;
  }

  if (!hasKeyword.value || !searchResults.value.length) {
    return [];
  }

  return [
    {
      key: "search-results",
      label: "Search Results",
      entries: searchResults.value,
    },
  ];
});

const searchEmptyTitle = computed(() => `No "${keyword.value.trim()}" results found`);

function contactMetaLine(contact: ContactItem) {
  return contact.subtitle ? `${contact.handle} · ${contact.subtitle}` : contact.handle;
}

function compareEntries(left: MessageListEntry, right: MessageListEntry) {
  const label = left.sortText.localeCompare(right.sortText, undefined, { sensitivity: "base" });
  if (label !== 0) {
    return label;
  }

  return left.id.localeCompare(right.id, undefined, { sensitivity: "base" });
}

function groupKeyForEntry(value: string) {
  const firstCharacter = value.trim().charAt(0).toUpperCase();
  return /^[A-Z]$/.test(firstCharacter) ? firstCharacter : "#";
}

function compareGroupKeys(left: string, right: string) {
  if (left === "#") {
    return 1;
  }

  if (right === "#") {
    return -1;
  }

  return left.localeCompare(right, undefined, { sensitivity: "base" });
}

function openEntry(entry: MessageListEntry) {
  if (entry.kind === "self") {
    emit("open-self-confirm");
    return;
  }

  emit("open-contact", entry.contact.id);
}

function selectEntry(entry: MessageListEntry) {
  if (entry.kind !== "contact") {
    return;
  }

  emit("select-contact", entry.contact.id);
}

function openInvitePage() {
  if (props.canInvite === false || !props.circle || !props.inviteLink.trim()) {
    return;
  }

  emit("open-invite");
}
</script>

<template>
  <OverlayPageShell
    title="New Message"
    :subtitle="circle ? `Start a chat inside ${circle.name}` : 'Choose how you want to begin a conversation.'"
    @close="emit('close')"
  >
    <div class="new-message-page">
      <section class="search-panel">
        <div class="search-field">
          <i class="pi pi-search"></i>
          <InputText
            v-model="keyword"
            placeholder="Search npub or username"
            @focus="searchFocused = true"
            @blur="searchFocused = false"
          />
        </div>

        <div v-if="!isSearchMode" class="search-meta">
          <span>{{ circle ? `Inside ${circle.name}` : "No active circle" }}</span>
          <span>{{ contacts.length }} contacts</span>
          <span>{{ currentCircleContactIds.length }} in circle</span>
        </div>
      </section>

      <section v-if="!isSearchMode" class="action-section">
        <button type="button" class="action-row" @click="emit('open-group-select')">
          <span class="action-icon">
            <i class="pi pi-users"></i>
          </span>
          <span class="action-copy">
            <strong>New Group</strong>
            <span>Create a group and choose members before you send the first message.</span>
          </span>
          <i class="pi pi-angle-right row-chevron"></i>
        </button>

        <button
          type="button"
          class="action-row"
          :disabled="props.canAddFriends === false"
          @click="props.canAddFriends === false || emit('open-find-people')"
        >
          <span class="action-icon">
            <i class="pi pi-user-plus"></i>
          </span>
          <span class="action-copy">
            <strong>Add Friends</strong>
            <span>Look up an invite link, handle or user ID to start a new chat.</span>
          </span>
          <i class="pi pi-angle-right row-chevron"></i>
        </button>

        <button
          type="button"
          class="action-row"
          :disabled="props.canInvite === false"
          @click="openInvitePage"
        >
          <span class="action-icon">
            <i class="pi pi-send"></i>
          </span>
          <span class="action-copy">
            <strong>Invite</strong>
            <span>Open the invite page to share, copy, or show the QR code for this circle.</span>
          </span>
          <i class="pi pi-angle-right row-chevron"></i>
        </button>
      </section>

      <section v-if="isSearchMode && !hasKeyword" class="list-section search-placeholder">
        <i class="pi pi-search"></i>
        <p>Search by name, handle or pubkey.</p>
      </section>

      <section v-else-if="isSearchMode && hasKeyword && !searchResults.length" class="empty-state">
        <i class="pi pi-user-plus"></i>
        <h3>{{ searchEmptyTitle }}</h3>
        <p>Try a different name, handle or pubkey.</p>
      </section>

      <section v-for="group in visibleGroups" :key="group.key" class="list-section">
        <div class="section-head">
          <div class="section-heading">{{ group.label }}</div>
          <span class="section-count">{{ group.entries.length }}</span>
        </div>

        <div class="contact-list">
          <div
            v-for="entry in group.entries"
            :key="entry.id"
            class="contact-row"
            :class="{ 'contact-row-single': entry.kind === 'self' }"
          >
            <button type="button" class="contact-main" @click="openEntry(entry)">
              <span v-if="entry.kind === 'self'" class="self-avatar">
                <SelfChatIcon />
              </span>
              <Avatar
                v-else
                :label="entry.contact.initials"
                shape="circle"
                class="contact-avatar"
              />

              <div class="contact-copy">
                <div class="contact-name-line">
                  <strong>{{ entry.name }}</strong>
                  <span v-if="entry.kind === 'self'" class="row-badge">You</span>
                  <span v-else-if="entry.contact.online" class="online-dot"></span>
                  <span
                    v-if="entry.kind === 'contact' && currentCircleContactSet.has(entry.contact.id)"
                    class="row-badge"
                  >
                    In Circle
                  </span>
                </div>
                <p>{{ entry.meta }}</p>
              </div>
            </button>

            <Button
              v-if="entry.kind === 'contact'"
              icon="pi pi-send"
              rounded
              text
              severity="secondary"
              aria-label="Start conversation"
              @click="selectEntry(entry)"
            />
          </div>
        </div>
      </section>
    </div>
  </OverlayPageShell>
</template>

<style scoped>
.new-message-page,
.search-panel,
.action-section,
.list-section,
.contact-list,
.contact-copy,
.action-copy,
.empty-state {
  display: grid;
}

.new-message-page {
  gap: 16px;
}

.search-panel,
.action-section,
.list-section,
.empty-state {
  padding: 20px;
  border-radius: 24px;
  background: #f8fbfd;
}

.search-panel,
.list-section,
.empty-state {
  gap: 14px;
}

.action-section,
.contact-list {
  gap: 0;
}

.search-field,
.search-meta,
.action-row,
.section-head,
.contact-row,
.contact-main,
.contact-name-line {
  display: flex;
  align-items: center;
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

.search-meta {
  flex-wrap: wrap;
  gap: 8px 14px;
  color: #70839d;
  font-size: 0.82rem;
}

.action-row {
  gap: 14px;
  width: 100%;
  padding: 14px 0;
  border: 0;
  background: transparent;
  text-align: left;
  cursor: pointer;
}

.action-row:disabled {
  cursor: default;
  opacity: 0.48;
}

.action-row + .action-row,
.contact-row + .contact-row {
  border-top: 1px solid #dde7f1;
}

.action-icon {
  flex-shrink: 0;
  display: grid;
  place-items: center;
  width: 42px;
  height: 42px;
  border-radius: 14px;
  background: #edf4fb;
  color: #30557f;
}

.action-copy,
.contact-copy {
  gap: 4px;
  min-width: 0;
}

.action-copy strong,
.action-copy span,
.contact-copy strong,
.contact-copy p,
.empty-state h3,
.empty-state p,
.search-placeholder p {
  margin: 0;
}

.action-copy span,
.contact-copy p,
.empty-state p,
.search-placeholder p {
  color: #6d809a;
}

.action-copy span {
  line-height: 1.5;
}

.row-chevron {
  margin-left: auto;
  color: #8da0b9;
}

.section-head {
  justify-content: space-between;
  gap: 12px;
}

.section-heading {
  color: #6a7d98;
  text-transform: uppercase;
  letter-spacing: 0.14em;
  font-size: 0.72rem;
}

.section-count,
.row-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 999px;
  background: #e9f0f8;
  color: #537195;
  font-size: 0.75rem;
  font-weight: 600;
}

.section-count {
  min-width: 28px;
  padding: 4px 9px;
}

.contact-row {
  gap: 10px;
  justify-content: space-between;
  padding: 14px 0;
}

.contact-row-single {
  justify-content: flex-start;
}

.contact-main {
  flex: 1;
  gap: 12px;
  min-width: 0;
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

.self-avatar {
  flex-shrink: 0;
  display: grid;
  place-items: center;
}

.contact-name-line {
  gap: 8px;
  min-width: 0;
}

.contact-name-line strong,
.contact-copy p {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.row-badge {
  flex-shrink: 0;
  padding: 3px 8px;
}

.online-dot {
  width: 8px;
  height: 8px;
  border-radius: 999px;
  background: #35c98d;
  flex-shrink: 0;
}

.empty-state,
.search-placeholder {
  justify-items: center;
  gap: 12px;
  text-align: center;
}

.empty-state i,
.search-placeholder i {
  font-size: 2rem;
  color: #7d8ea6;
}
</style>
