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
  return props.group?.members.find((member) => member.contactId === contactId)?.role === "admin"
    ? "Admin"
    : "Member";
}
</script>

<template>
  <OverlayPageShell
    title="Group Info"
    subtitle="Members, mute state and group controls."
    @close="emit('close')"
  >
    <div v-if="session && group" class="group-body">
      <section class="hero-card">
        <Avatar :label="session.initials" shape="circle" class="hero-avatar" />
        <h2>{{ group.name }}</h2>
        <p>{{ group.description }}</p>
        <Tag :value="`${memberContacts.length} Members`" severity="secondary" rounded />
      </section>

      <section class="section-card">
        <div class="section-title">Group Name</div>
        <div class="info-row">
          <strong>{{ group.name }}</strong>
          <span>{{ group.muted ? "Muted" : "Active" }}</span>
        </div>
      </section>

      <section class="section-card">
        <div class="section-head">
          <div class="section-title">Members</div>
          <Button
            v-if="hasMoreMembers"
            :label="showAllMembers ? 'Show Less' : 'See All'"
            text
            severity="contrast"
            @click="showAllMembers = !showAllMembers"
          />
        </div>

        <div class="member-list">
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
    </div>

    <div v-else class="missing-state">
      <i class="pi pi-users"></i>
      <p>This group is no longer available.</p>
    </div>

    <template v-if="session && group" #footer>
      <div class="group-actions">
        <Button
          :icon="group.muted ? 'pi pi-volume-up' : 'pi pi-volume-off'"
          :label="group.muted ? 'Unmute Group' : 'Mute Group'"
          text
          severity="contrast"
          @click="emit('toggle-mute', session.id)"
        />
        <Button
          icon="pi pi-sign-out"
          label="Leave Group"
          severity="danger"
          text
          @click="emit('leave-group', session.id)"
        />
      </div>
    </template>
  </OverlayPageShell>
</template>

<style scoped>
.group-body,
.section-card,
.member-list {
  display: grid;
}

.group-body {
  gap: 18px;
}

.hero-card {
  display: grid;
  justify-items: center;
  gap: 10px;
  padding: 28px 24px;
  border-radius: 28px;
  background:
    radial-gradient(circle at top left, rgba(106, 168, 255, 0.18), transparent 26%),
    linear-gradient(180deg, #f7fbfe 0%, #f2f7fb 100%);
  text-align: center;
}

.hero-avatar,
.member-avatar {
  background: linear-gradient(135deg, #dce9ff 0%, #d9f9ef 100%);
  color: #16355c;
  font-weight: 700;
}

.hero-avatar {
  width: 92px;
  height: 92px;
  font-size: 1.45rem;
}

.hero-card h2,
.hero-card p,
.missing-state p {
  margin: 0;
}

.hero-card p {
  max-width: 44ch;
  color: #6d809a;
  line-height: 1.65;
}

.section-card {
  gap: 12px;
}

.section-head,
.group-actions,
.info-row,
.member-row {
  display: flex;
  align-items: center;
}

.section-head {
  justify-content: space-between;
  gap: 12px;
}

.section-title {
  color: #6a7d98;
  text-transform: uppercase;
  letter-spacing: 0.14em;
  font-size: 0.72rem;
}

.info-row {
  justify-content: space-between;
  gap: 12px;
  padding: 16px 18px;
  border-radius: 20px;
  background: #f7fafc;
}

.info-row span {
  color: #6d809a;
}

.member-list {
  gap: 10px;
}

.member-row {
  gap: 12px;
  width: 100%;
  padding: 14px 12px;
  border: 0;
  border-radius: 20px;
  background: #f7fafc;
  text-align: left;
  cursor: pointer;
}

.member-row:hover {
  background: #f2f7fb;
}

.member-avatar {
  width: 42px;
  height: 42px;
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
  color: #71839d;
}

.group-actions {
  gap: 10px;
  flex-wrap: wrap;
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
</style>
