<script setup lang="ts">
import { computed } from "vue";
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import Drawer from "primevue/drawer";
import Tag from "primevue/tag";
import SelfChatIcon from "./SelfChatIcon.vue";
import type { ContactItem, GroupProfile, SessionItem } from "../types/chat";

const props = defineProps<{
  visible: boolean;
  session: SessionItem | null;
  contact: ContactItem | null;
  group: GroupProfile | null;
  memberContacts: ContactItem[];
}>();

const emit = defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "toggle-block", contactId: string): void;
  (event: "toggle-mute-group", sessionId: string): void;
  (event: "leave-group", sessionId: string): void;
}>();

const drawerVisible = computed({
  get: () => props.visible,
  set: (value: boolean) => emit("update:visible", value),
});

const isDirect = computed(() => props.session?.kind === "direct");
const isGroup = computed(() => props.session?.kind === "group");
const isSelf = computed(() => props.session?.kind === "self");

const titleText = computed(() => {
  if (!props.session) {
    return "Details";
  }

  if (isGroup.value) {
    return "Group Info";
  }

  if (isSelf.value) {
    return "Chat Info";
  }

  return "User Detail";
});
</script>

<template>
  <Drawer
    v-model:visible="drawerVisible"
    position="right"
    class="details-drawer"
    :dismissable="true"
    :modal="true"
  >
    <template #header>
      <div class="drawer-header">{{ titleText }}</div>
    </template>

    <div v-if="session" class="details-body">
      <section class="hero-card">
        <SelfChatIcon v-if="isSelf" size="lg" />
        <Avatar
          v-else
          :label="session.initials"
          shape="circle"
          class="hero-avatar"
        />

        <div class="hero-copy">
          <h2>{{ session.name }}</h2>
          <p v-if="isGroup">{{ group?.description }}</p>
          <template v-else-if="contact">
            <span>{{ contact.handle }}</span>
            <span class="pubkey">{{ contact.pubkey }}</span>
          </template>
          <template v-else>
            <span>Notes, links and files for this circle.</span>
          </template>
        </div>
      </section>

      <section v-if="isDirect && contact" class="drawer-section">
        <p class="section-title">Profile</p>
        <div class="info-card">
          <div class="info-row">
            <span class="label">Status</span>
            <Tag :value="contact.online ? 'Online' : 'Offline'" :severity="contact.online ? 'success' : 'secondary'" rounded />
          </div>
          <div class="info-row block">
            <span class="label">Bio</span>
            <p>{{ contact.bio }}</p>
          </div>
        </div>
        <Button
          :icon="contact.blocked ? 'pi pi-lock-open' : 'pi pi-ban'"
          :label="contact.blocked ? 'Unblock User' : 'Block User'"
          text
          severity="danger"
          @click="emit('toggle-block', contact.id)"
        />
      </section>

      <section v-if="isSelf" class="drawer-section">
        <p class="section-title">Assistant</p>
        <div class="info-card">
          <div class="info-row block">
            <span class="label">Usage</span>
            <p>Use this chat to keep notes, links and files for the current circle.</p>
          </div>
          <div class="info-row block">
            <span class="label">Scope</span>
            <p>Anything saved here stays tied to this circle.</p>
          </div>
        </div>
      </section>

      <section v-if="isGroup && group" class="drawer-section">
        <p class="section-title">Members</p>
        <div class="member-list">
          <div
            v-for="member in memberContacts"
            :key="member.id"
            class="member-row"
          >
            <Avatar :label="member.initials" shape="circle" class="member-avatar" />
            <div class="member-copy">
              <strong>{{ member.name }}</strong>
              <span>{{ member.handle }}</span>
            </div>
            <Tag
              :value="group.members.find((item) => item.contactId === member.id)?.role === 'admin' ? 'Admin' : 'Member'"
              severity="secondary"
              rounded
            />
          </div>
        </div>

        <div class="group-actions">
          <Button
            :icon="group.muted ? 'pi pi-volume-up' : 'pi pi-volume-off'"
            :label="group.muted ? 'Unmute Group' : 'Mute Group'"
            text
            severity="contrast"
            @click="emit('toggle-mute-group', session.id)"
          />
          <Button
            icon="pi pi-sign-out"
            label="Leave Group"
            text
            severity="danger"
            @click="emit('leave-group', session.id)"
          />
        </div>
      </section>
    </div>
  </Drawer>
</template>

<style scoped>
.drawer-header {
  font-weight: 700;
}

.details-body {
  display: grid;
  gap: 20px;
}

.hero-card,
.info-row,
.member-row,
.group-actions {
  display: flex;
  align-items: center;
}

.hero-card {
  gap: 14px;
  padding: 16px;
  border-radius: 22px;
  background: #f5f8fb;
}

.hero-avatar,
.member-avatar {
  background: linear-gradient(135deg, #dce9ff 0%, #d9f9ef 100%);
  color: #16355c;
  font-weight: 700;
}

.hero-avatar {
  width: 48px;
  height: 48px;
}

.member-avatar {
  width: 36px;
  height: 36px;
}

.hero-copy {
  display: grid;
  gap: 4px;
  min-width: 0;
}

.hero-copy h2,
.hero-copy p,
.hero-copy span,
.section-title {
  margin: 0;
}

.hero-copy p,
.hero-copy span {
  color: #6d809a;
}

.pubkey {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.drawer-section {
  display: grid;
  gap: 10px;
}

.section-title {
  color: #6a7d98;
  text-transform: uppercase;
  letter-spacing: 0.16em;
  font-size: 0.72rem;
}

.info-card,
.member-list {
  display: grid;
  gap: 8px;
}

.info-row,
.member-row {
  gap: 12px;
  padding: 12px 10px;
  border-radius: 16px;
  background: #f8fafc;
}

.info-row {
  justify-content: space-between;
}

.info-row.block {
  display: grid;
  justify-content: stretch;
}

.info-row.block p {
  margin: 6px 0 0;
  color: #5f738d;
  line-height: 1.6;
}

.label {
  color: #72839b;
  font-size: 0.86rem;
}

.member-copy {
  display: grid;
  gap: 4px;
  min-width: 0;
  flex: 1;
}

.member-copy strong,
.member-copy span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.member-copy span {
  color: #72839d;
  font-size: 0.84rem;
}

.group-actions {
  gap: 8px;
  flex-wrap: wrap;
}

:deep(.details-drawer) {
  width: min(420px, 100vw);
}
</style>
