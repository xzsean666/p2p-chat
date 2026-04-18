<script setup lang="ts">
import Avatar from "primevue/avatar";
import Tag from "primevue/tag";
import OverlayPageShell from "./OverlayPageShell.vue";
import type { ContactItem, GroupProfile, SessionItem } from "../types/chat";

defineProps<{
  session: SessionItem | null;
  group: GroupProfile | null;
  memberContacts: ContactItem[];
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "open-member", contactId: string): void;
}>();
</script>

<template>
  <OverlayPageShell
    title="Group Members"
    subtitle="Browse the full member list and open individual profiles."
    @close="emit('close')"
  >
    <div v-if="session && group" class="members-body">
      <section class="hero-card">
        <h3>{{ group.name }}</h3>
        <p>{{ memberContacts.length }} visible members in this desktop rebuild.</p>
      </section>

      <section class="list-card">
        <button
          v-for="member in memberContacts"
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
          <Tag
            :value="group.members.find((item) => item.contactId === member.id)?.role === 'admin' ? 'Admin' : 'Member'"
            severity="secondary"
            rounded
          />
        </button>
      </section>
    </div>

    <div v-else class="missing-state">
      <i class="pi pi-users"></i>
      <p>This group is no longer available.</p>
    </div>
  </OverlayPageShell>
</template>

<style scoped>
.members-body,
.hero-card,
.list-card,
.member-copy,
.missing-state {
  display: grid;
}

.members-body {
  gap: 18px;
}

.hero-card {
  gap: 6px;
  padding: 22px;
  border-radius: 26px;
  background:
    radial-gradient(circle at top left, rgba(106, 168, 255, 0.18), transparent 30%),
    linear-gradient(180deg, #f7fbfe 0%, #f2f7fb 100%);
}

.hero-card h3,
.hero-card p,
.missing-state p {
  margin: 0;
}

.hero-card p {
  color: #6d809a;
}

.list-card {
  gap: 10px;
}

.member-row {
  display: flex;
  align-items: center;
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
  background: linear-gradient(135deg, #dce9ff 0%, #d9f9ef 100%);
  color: #16355c;
  font-weight: 700;
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
  justify-items: center;
  align-content: center;
  min-height: 100%;
  gap: 10px;
  color: #6d809a;
}

.missing-state i {
  font-size: 2rem;
}
</style>
