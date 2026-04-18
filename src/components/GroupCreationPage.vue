<script setup lang="ts">
import { computed, ref, watch } from "vue";
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import Tag from "primevue/tag";
import OverlayPageShell from "./OverlayPageShell.vue";
import type { CircleItem, ContactItem, UserProfile } from "../types/chat";

const props = defineProps<{
  circle: CircleItem | null;
  userProfile: UserProfile;
  memberContacts: ContactItem[];
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "create-group", payload: { name: string; memberContactIds: string[] }): void;
  (event: "open-member", contactId: string): void;
}>();

const groupName = ref("");

const defaultGroupName = computed(() => {
  const ownerName = props.userProfile.name.trim() || "My";
  if (props.memberContacts.length === 1) {
    return `${props.memberContacts[0].name} & ${ownerName}`;
  }

  const possessiveOwner = ownerName.endsWith("s") ? `${ownerName}'` : `${ownerName}'s`;
  return `${possessiveOwner} Group`;
});

watch(
  () => defaultGroupName.value,
  (value) => {
    groupName.value = value;
  },
  { immediate: true },
);

const canCreate = computed(() => {
  return props.memberContacts.length > 0 && !!groupName.value.trim();
});

function submit() {
  if (!canCreate.value) {
    return;
  }

  emit("create-group", {
    name: groupName.value.trim(),
    memberContactIds: props.memberContacts.map((contact) => contact.id),
  });
}
</script>

<template>
  <OverlayPageShell
    title="Create Group"
    :subtitle="circle ? `Finalize the new group inside ${circle.name}` : 'Finalize the new group setup.'"
    @close="emit('close')"
  >
    <div class="group-create-body">
      <section class="group-info-card">
        <div class="avatar-shell">
          <div class="avatar-placeholder">
            <i class="pi pi-camera"></i>
          </div>
        </div>

        <div class="group-copy">
          <label class="field-label" for="group-name-input">Group Name</label>
          <InputText
            id="group-name-input"
            v-model="groupName"
            placeholder="Launch Crew"
            class="name-input"
            @keyup.enter="submit"
          />
          <p>
            The original app lets you confirm the title after selecting members. This desktop flow now mirrors that split.
          </p>
        </div>
      </section>

      <section class="members-card">
        <div class="section-head">
          <div>
            <div class="section-title">Members</div>
            <p>{{ memberContacts.length + 1 }} visible participants including you.</p>
          </div>
          <Tag :value="`${memberContacts.length + 1} total`" severity="contrast" rounded />
        </div>

        <div class="member-list">
          <div class="member-row self-row">
            <Avatar :label="userProfile.initials" shape="circle" class="member-avatar self-avatar" />
            <div class="member-copy">
              <strong>{{ userProfile.name }}</strong>
              <span>{{ userProfile.handle || "Current user" }}</span>
            </div>
            <Tag value="Admin" severity="secondary" rounded />
          </div>

          <button
            v-for="contact in memberContacts"
            :key="contact.id"
            type="button"
            class="member-row"
            @click="emit('open-member', contact.id)"
          >
            <Avatar :label="contact.initials" shape="circle" class="member-avatar" />
            <div class="member-copy">
              <strong>{{ contact.name }}</strong>
              <span>{{ contact.handle }}</span>
            </div>
            <Tag value="Member" severity="secondary" rounded />
          </button>
        </div>
      </section>
    </div>

    <template #footer>
      <div class="footer-actions">
        <Button label="Back" text severity="secondary" @click="emit('close')" />
        <Button
          label="Create Group"
          icon="pi pi-check"
          severity="contrast"
          :disabled="!canCreate"
          @click="submit"
        />
      </div>
    </template>
  </OverlayPageShell>
</template>

<style scoped>
.group-create-body,
.group-info-card,
.group-copy,
.members-card,
.member-list,
.footer-actions {
  display: grid;
}

.group-create-body {
  gap: 18px;
}

.group-info-card,
.members-card {
  gap: 16px;
  padding: 22px;
  border-radius: 28px;
}

.group-info-card {
  grid-template-columns: auto minmax(0, 1fr);
  background:
    radial-gradient(circle at top left, rgba(106, 168, 255, 0.18), transparent 28%),
    linear-gradient(180deg, #f7fbfe 0%, #f2f7fb 100%);
}

.members-card {
  background: #f8fbfd;
}

.avatar-shell {
  display: flex;
  align-items: flex-start;
}

.avatar-placeholder {
  display: grid;
  place-items: center;
  width: 72px;
  height: 72px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.86);
  color: #46678f;
  font-size: 1.15rem;
}

.group-copy {
  gap: 10px;
  min-width: 0;
}

.field-label,
.section-title {
  color: #6a7d98;
  text-transform: uppercase;
  letter-spacing: 0.14em;
  font-size: 0.72rem;
  font-weight: 700;
}

.name-input {
  width: 100%;
}

.group-copy p,
.section-head p,
.member-copy span {
  margin: 0;
  color: #6d809a;
  line-height: 1.6;
}

.section-head,
.member-row,
.footer-actions {
  display: flex;
  align-items: center;
}

.section-head {
  justify-content: space-between;
  gap: 12px;
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
  background: #ffffff;
  text-align: left;
  cursor: pointer;
}

.member-row.self-row {
  cursor: default;
}

.member-avatar {
  width: 42px;
  height: 42px;
  background: linear-gradient(135deg, #dce9ff 0%, #d9f9ef 100%);
  color: #16355c;
  font-weight: 700;
}

.self-avatar {
  background: linear-gradient(135deg, #ffe3e3 0%, #fff3d6 100%);
  color: #7a2f2f;
}

.member-copy {
  min-width: 0;
  flex: 1;
}

.member-copy strong,
.member-copy span {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.footer-actions {
  justify-content: end;
  gap: 10px;
}

@media (max-width: 720px) {
  .group-info-card {
    grid-template-columns: 1fr;
  }

  .footer-actions {
    flex-direction: column;
  }
}
</style>
