<script setup lang="ts">
import { computed, ref } from "vue";
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import Tag from "primevue/tag";
import OverlayPageShell from "./OverlayPageShell.vue";
import type { ContactItem, GroupProfile, SessionItem } from "../types/chat";

const props = defineProps<{
  session: SessionItem | null;
  group: GroupProfile | null;
  memberContacts: ContactItem[];
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "open-member", contactId: string): void;
  (event: "open-members", sessionId: string): void;
  (event: "edit-name", sessionId: string): void;
  (event: "add-members", sessionId: string): void;
  (event: "remove-members", sessionId: string): void;
  (event: "toggle-mute", sessionId: string): void;
  (event: "leave-group", sessionId: string): void;
}>();

const showAllMembers = ref(false);

const displayedMembers = computed(() => {
  if (showAllMembers.value) {
    return props.memberContacts;
  }

  return props.memberContacts.slice(0, 5);
});

const hasMoreMembers = computed(() => props.memberContacts.length > 5);

function memberRole(contactId: string) {
  return props.group?.members.find((member) => member.contactId === contactId)?.role === "admin" ? "Admin" : "Member";
}
</script>

<template>
  <OverlayPageShell title="Group Info" subtitle="Members" @close="emit('close')">
    <div v-if="session && group" class="group-page">
      <section class="group-header">
        <Avatar :label="session.initials" shape="circle" class="group-avatar" />
        <h2>{{ group.name }}</h2>
        <p>{{ group.description || "No group description available." }}</p>
        <div class="group-tags">
          <Tag :value="`${memberContacts.length} Members`" severity="secondary" rounded />
          <Tag :value="group.muted ? 'Muted' : 'Active'" :severity="group.muted ? 'secondary' : 'success'" rounded />
        </div>
      </section>

      <section class="info-section">
        <div class="section-head">
          <div class="section-title">Group</div>
          <Button
            label="Edit Name"
            text
            severity="contrast"
            @click="emit('edit-name', session.id)"
          />
        </div>
        <div class="section-list">
          <div class="info-row">
            <strong>Group Name</strong>
            <span>{{ group.name }}</span>
          </div>
          <div class="info-row">
            <strong>Status</strong>
            <span>{{ group.muted ? "Muted" : "Active" }}</span>
          </div>
        </div>
      </section>

      <section class="info-section">
        <div class="section-head">
          <div class="section-title">Members</div>
          <Button
            label="See All"
            text
            severity="contrast"
            @click="emit('open-members', session.id)"
          />
        </div>

        <div class="section-list action-list">
          <Button
            label="Add Members"
            text
            severity="contrast"
            class="action-button"
            @click="emit('add-members', session.id)"
          />
          <Button
            v-if="memberContacts.length > 1"
            label="Remove Members"
            text
            severity="contrast"
            class="action-button"
            @click="emit('remove-members', session.id)"
          />
          <Button
            v-if="hasMoreMembers"
            :label="showAllMembers ? 'Show Less' : 'Preview More'"
            text
            severity="secondary"
            class="action-button"
            @click="showAllMembers = !showAllMembers"
          />
        </div>

        <div class="section-list member-list">
          <button
            v-for="member in displayedMembers"
            :key="member.id"
            type="button"
            class="member-row"
            @click="emit('open-member', member.id)"
          >
            <Avatar :label="member.initials" shape="circle" class="member-avatar" />
            <div class="member-copy">
              <strong>{{ member.name }}</strong>
              <span>{{ member.handle }}</span>
            </div>
            <Tag :value="memberRole(member.id)" severity="secondary" rounded />
          </button>
        </div>
      </section>

      <section class="info-section">
        <div class="section-title">Actions</div>
        <div class="section-list action-list">
          <Button
            :icon="group.muted ? 'pi pi-volume-up' : 'pi pi-volume-off'"
            :label="group.muted ? 'Unmute Group' : 'Mute Group'"
            text
            severity="contrast"
            class="action-button"
            @click="emit('toggle-mute', session.id)"
          />
          <Button
            icon="pi pi-sign-out"
            label="Leave Group"
            severity="danger"
            text
            class="action-button"
            @click="emit('leave-group', session.id)"
          />
        </div>
      </section>
    </div>

    <div v-else class="missing-state">
      <i class="pi pi-users"></i>
      <p>This group is no longer available.</p>
    </div>
  </OverlayPageShell>
</template>

<style scoped>
.group-page,
.group-header,
.info-section,
.section-list,
.member-copy {
  display: grid;
}

.group-page {
  gap: 22px;
  padding-top: 2px;
}

.group-header {
  justify-items: center;
  gap: 8px;
  padding: 8px 0 2px;
  text-align: center;
}

.group-avatar,
.member-avatar {
  background: #edf2f7;
  color: #2b4968;
  font-weight: 700;
}

.group-avatar {
  width: 76px;
  height: 76px;
  font-size: 1.2rem;
}

.group-header h2,
.group-header p,
.missing-state p {
  margin: 0;
}

.group-header h2 {
  font-size: 1.28rem;
  font-weight: 600;
  color: #23364a;
}

.group-header p {
  max-width: 34rem;
  color: #6d809a;
  line-height: 1.6;
}

.group-tags,
.section-head {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.group-tags {
  justify-content: center;
  margin-top: 4px;
}

.section-head {
  justify-content: space-between;
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

.info-row,
.member-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 15px 16px;
  border-bottom: 1px solid #e7ebf1;
}

.info-row {
  justify-content: space-between;
}

.info-row:last-child,
.member-row:last-child,
.action-list :deep(.p-button:last-child) {
  border-bottom: 0;
}

.info-row span {
  color: #6d809a;
}

.action-list :deep(.p-button) {
  justify-content: flex-start;
  width: 100%;
  padding: 15px 16px;
  border-bottom: 1px solid #e7ebf1;
  border-radius: 0;
}

.member-row {
  width: 100%;
  border-inline: 0;
  background: transparent;
  text-align: left;
  cursor: pointer;
}

.member-avatar {
  width: 42px;
  height: 42px;
}

.member-copy {
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
  color: #71839d;
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
  .section-head {
    align-items: flex-start;
  }
}
</style>
