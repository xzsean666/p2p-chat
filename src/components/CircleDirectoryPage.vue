<script setup lang="ts">
import { computed, ref } from "vue";
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import Divider from "primevue/divider";
import InputText from "primevue/inputtext";
import Tag from "primevue/tag";
import OverlayPageShell from "./OverlayPageShell.vue";
import type {
  CircleItem,
  CircleTransportDiagnostic,
  CircleType,
  RestorableCircleEntry,
  TransportActivityItem,
  TransportSnapshot,
} from "../types/chat";

type JoinMode = "invite" | "private" | "custom";

const props = defineProps<{
  circles: CircleItem[];
  activeCircleId: string;
  restorableCircles: RestorableCircleEntry[];
  transportSnapshot: TransportSnapshot | null;
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "select-circle", circleId: string): void;
  (event: "open-circle-detail", circleId: string): void;
  (event: "restore-circle", entry: RestorableCircleEntry): void;
  (event: "forget-restorable-circle", relay: string): void;
  (
    event: "add-circle",
    payload: {
      mode: JoinMode;
      name: string;
      relay?: string;
      inviteCode?: string;
    },
  ): void;
}>();

const joinMode = ref<JoinMode>("invite");
const circleName = ref("");
const relayValue = ref("");
const inviteCode = ref("");

const openCount = computed(() => {
  return props.circles.filter((circle) => circle.status === "open").length;
});

const statusLabel = computed(() => {
  if (props.circles.length === 0 && props.restorableCircles.length > 0) {
    return `0 circles · ${props.restorableCircles.length} saved`;
  }

  return `${props.circles.length} circles · ${openCount.value} online`;
});

const directorySubtitle = computed(() => {
  if (props.circles.length > 0) {
    return "Switch circles, restore a saved relay space, or connect a new one.";
  }

  if (props.restorableCircles.length > 0) {
    return "Restore a saved circle or connect a new relay space before starting chats.";
  }

  return "Connect a new relay space before starting chats in this shell.";
});

const heroDescription = computed(() => {
  if (props.circles.length > 0) {
    return "Keep the current circle quick-switch overlay for fast hops, and use this page for fuller setup.";
  }

  if (props.restorableCircles.length > 0) {
    return "Nothing is mounted right now. Restore a saved circle entry or connect a new relay space from this page.";
  }

  return "Start by joining or creating a circle from this page, then return to the chat shell.";
});

const currentCirclesEmptyTitle = computed(() => {
  return props.restorableCircles.length > 0 ? "No circles mounted right now" : "No circles in this shell";
});

const currentCirclesEmptyCopy = computed(() => {
  return props.restorableCircles.length > 0
    ? "You can restore one of the saved circle entries below, or connect a new relay space from this directory."
    : "Join or create a circle from this directory before starting chats.";
});

const addSectionTitle = computed(() => {
  return props.circles.length > 0 ? "Add Circle" : "Join or Add Circle";
});

const diagnosticByCircleId = computed(() => {
  return new Map(
    (props.transportSnapshot?.diagnostics ?? []).map((diagnostic) => [diagnostic.circleId, diagnostic]),
  );
});

const latestActivityByCircleId = computed(() => {
  const map = new Map<string, TransportActivityItem>();

  for (const item of props.transportSnapshot?.activities ?? []) {
    if (!map.has(item.circleId) && item.kind !== "runtime") {
      map.set(item.circleId, item);
    }
  }

  return map;
});

const canSubmit = computed(() => {
  if (joinMode.value === "invite") {
    return inviteCode.value.trim().length > 0;
  }

  if (joinMode.value === "private") {
    return circleName.value.trim().length > 0;
  }

  return circleName.value.trim().length > 0 && relayValue.value.trim().length > 0;
});

const submitLabel = computed(() => {
  if (joinMode.value === "private") {
    return "Create Private Circle";
  }

  return "Connect Circle";
});

function typeLabel(type: CircleType) {
  switch (type) {
    case "paid":
      return "Private";
    case "bitchat":
      return "Offline";
    case "custom":
      return "Custom";
    default:
      return "Invite";
  }
}

function typeTone(type: CircleType) {
  switch (type) {
    case "paid":
      return "warn";
    case "bitchat":
      return "secondary";
    case "custom":
      return "contrast";
    default:
      return "success";
  }
}

function statusTone(status: CircleItem["status"]) {
  if (status === "open") {
    return "success";
  }

  if (status === "connecting") {
    return "warn";
  }

  return "secondary";
}

function transportTone(health: CircleTransportDiagnostic["health"]) {
  if (health === "online") {
    return "success";
  }

  if (health === "degraded") {
    return "warn";
  }

  return "secondary";
}

function circleDiagnostic(circleId: string) {
  return diagnosticByCircleId.value.get(circleId) ?? null;
}

function circleLatestActivity(circleId: string) {
  return latestActivityByCircleId.value.get(circleId) ?? null;
}

function archivedAtCopy(value: string) {
  const time = new Date(value);
  if (Number.isNaN(time.getTime())) {
    return value;
  }

  return time.toLocaleString();
}

function submit() {
  if (!canSubmit.value) {
    return;
  }

  emit("add-circle", {
    mode: joinMode.value,
    name: circleName.value.trim(),
    relay: relayValue.value.trim(),
    inviteCode: inviteCode.value.trim(),
  });

  circleName.value = "";
  relayValue.value = "";
  inviteCode.value = "";
  joinMode.value = "invite";
}
</script>

<template>
  <OverlayPageShell
    title="Circle Directory"
    :subtitle="directorySubtitle"
    @close="emit('close')"
  >
    <div class="directory-page">
      <section class="hero-card">
        <div class="hero-copy">
          <h3>Circles</h3>
          <p>{{ heroDescription }}</p>
        </div>
        <div class="hero-tags">
          <Tag :value="statusLabel" severity="info" rounded />
          <Tag
            v-if="transportSnapshot"
            :value="transportSnapshot.engine"
            severity="contrast"
            rounded
          />
        </div>
      </section>

      <section class="section-card">
        <div class="section-head">
          <span class="section-title">Current Circles</span>
        </div>

        <div v-if="circles.length" class="circle-list">
          <div
            v-for="circle in circles"
            :key="circle.id"
            :class="['circle-row-shell', { active: circle.id === activeCircleId }]"
          >
            <button
              type="button"
              class="circle-row"
              @click="emit('select-circle', circle.id)"
            >
              <Avatar :label="circle.name.slice(0, 1)" shape="circle" class="circle-avatar" />
              <div class="circle-copy">
                <div class="row-head">
                  <strong>{{ circle.name }}</strong>
                  <div class="row-tags">
                    <Tag :value="typeLabel(circle.type)" :severity="typeTone(circle.type)" rounded />
                    <Tag :value="circle.status" :severity="statusTone(circle.status)" rounded />
                  </div>
                </div>
                <p>{{ circle.description }}</p>
                <div class="row-meta">
                  <span>{{ circle.relay }}</span>
                  <span>{{ circle.latency }}</span>
                </div>
                <div v-if="circleDiagnostic(circle.id)" class="transport-meta">
                  <Tag
                    :value="circleDiagnostic(circle.id)?.health"
                    :severity="transportTone(circleDiagnostic(circle.id)!.health)"
                    rounded
                  />
                  <span>
                    {{ circleDiagnostic(circle.id)?.protocol }} ·
                    {{ circleDiagnostic(circle.id)?.peerCount }} peers ·
                    {{ circleLatestActivity(circle.id)?.title ?? circleDiagnostic(circle.id)?.lastSync }}
                  </span>
                </div>
              </div>
            </button>

            <Button
              icon="pi pi-info-circle"
              text
              rounded
              severity="secondary"
              class="circle-detail-button"
              @click="emit('open-circle-detail', circle.id)"
            />
          </div>
        </div>
        <div v-else class="empty-state-card">
          <strong>{{ currentCirclesEmptyTitle }}</strong>
          <p>{{ currentCirclesEmptyCopy }}</p>
          <div class="empty-tags">
            <Tag v-if="restorableCircles.length" :value="`${restorableCircles.length} restore entries`" severity="warn" rounded />
            <Tag :value="restorableCircles.length ? 'Restore ready' : 'Join required'" severity="contrast" rounded />
          </div>
        </div>
      </section>

      <section v-if="restorableCircles.length" class="section-card">
        <div class="section-head">
          <span class="section-title">Restore Access</span>
          <Tag :value="`${restorableCircles.length} saved`" severity="warn" rounded />
        </div>

        <div class="restore-list">
          <div v-for="entry in restorableCircles" :key="entry.relay" class="restore-row">
            <div class="restore-copy">
              <div class="row-head">
                <strong>{{ entry.name }}</strong>
                <div class="row-tags">
                  <Tag :value="typeLabel(entry.type)" :severity="typeTone(entry.type)" rounded />
                </div>
              </div>
              <p>{{ entry.description || "No description archived for this circle." }}</p>
              <div class="row-meta">
                <span>{{ entry.relay }}</span>
                <span>Archived {{ archivedAtCopy(entry.archivedAt) }}</span>
              </div>
            </div>

            <div class="restore-actions">
              <Button
                icon="pi pi-refresh"
                label="Restore"
                severity="contrast"
                @click="emit('restore-circle', entry)"
              />
              <Button
                icon="pi pi-trash"
                text
                severity="secondary"
                aria-label="Forget restore entry"
                @click="emit('forget-restorable-circle', entry.relay)"
              />
            </div>
          </div>
        </div>
      </section>

      <Divider />

      <section class="section-card">
        <div class="section-head">
          <span class="section-title">{{ addSectionTitle }}</span>
        </div>

        <div class="mode-grid">
          <button
            type="button"
            :class="['mode-card', { active: joinMode === 'invite' }]"
            @click="joinMode = 'invite'"
          >
            <i class="pi pi-link"></i>
            <strong>Invite</strong>
            <span>Use an invite code or handoff token.</span>
          </button>

          <button
            type="button"
            :class="['mode-card', { active: joinMode === 'private' }]"
            @click="joinMode = 'private'"
          >
            <i class="pi pi-lock"></i>
            <strong>Private Cloud</strong>
            <span>Spin up a private paid circle shell.</span>
          </button>

          <button
            type="button"
            :class="['mode-card', { active: joinMode === 'custom' }]"
            @click="joinMode = 'custom'"
          >
            <i class="pi pi-globe"></i>
            <strong>Custom Relay</strong>
            <span>Connect to a relay you already control.</span>
          </button>
        </div>

        <div class="form-grid">
          <label v-if="joinMode !== 'invite'" class="form-field">
            <span>Circle Name</span>
            <InputText
              v-model="circleName"
              :placeholder="joinMode === 'private' ? 'Studio Circle' : 'Team Relay'"
            />
          </label>

          <label v-if="joinMode === 'invite'" class="form-field">
            <span>Invite Code</span>
            <InputText v-model="inviteCode" placeholder="p2pchat://circle/invite-code" />
          </label>

          <label v-if="joinMode === 'custom'" class="form-field">
            <span>Relay Address</span>
            <InputText v-model="relayValue" placeholder="wss://relay.example.com" />
          </label>

          <div class="hint-card">
            <strong v-if="joinMode === 'invite'">Invite Flow</strong>
            <strong v-else-if="joinMode === 'private'">Private Flow</strong>
            <strong v-else>Custom Flow</strong>
            <p v-if="joinMode === 'invite'">
              The invite path creates a relay entry in connecting state so the shell can mimic onboarding.
            </p>
            <p v-else-if="joinMode === 'private'">
              Private circles come in as paid relays and land in a guarded, connecting state first.
            </p>
            <p v-else>
              Custom relays stay explicit: you provide the label and relay URL, then switch into the empty shell.
            </p>
          </div>
        </div>
      </section>
    </div>

    <template #footer>
      <div class="footer-actions">
        <Button label="Cancel" text severity="secondary" @click="emit('close')" />
        <Button
          icon="pi pi-plus"
          :label="submitLabel"
          severity="contrast"
          :disabled="!canSubmit"
          @click="submit"
        />
      </div>
    </template>
  </OverlayPageShell>
</template>

<style scoped>
.directory-page,
.hero-card,
.section-card,
.circle-list,
.restore-list,
.mode-grid,
.form-grid {
  display: grid;
}

.directory-page {
  gap: 20px;
}

.hero-card {
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 16px;
  align-items: start;
  padding: 24px;
  border-radius: 28px;
  background:
    radial-gradient(circle at top left, rgba(106, 168, 255, 0.18), transparent 26%),
    linear-gradient(180deg, #f7fbfe 0%, #f2f7fb 100%);
}

.hero-copy h3,
.hero-copy p,
.section-title,
.mode-card strong,
.mode-card span,
.hint-card strong,
.hint-card p {
  margin: 0;
}

.hero-copy {
  display: grid;
  gap: 8px;
}

.hero-copy p {
  color: #6b7d97;
  line-height: 1.65;
}

.section-card {
  gap: 14px;
}

.empty-state-card {
  display: grid;
  gap: 10px;
  padding: 18px;
  border-radius: 22px;
  background: #f7fafc;
}

.section-head,
.hero-tags,
.empty-tags,
.restore-actions {
  display: flex;
  align-items: center;
  gap: 12px;
}

.section-head {
  justify-content: space-between;
}

.section-title {
  color: #6a7d98;
  text-transform: uppercase;
  letter-spacing: 0.14em;
  font-size: 0.72rem;
}

.circle-list {
  gap: 10px;
}

.restore-list {
  gap: 10px;
}

.circle-row-shell {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 8px;
  align-items: stretch;
  border-radius: 20px;
  background: #f7fafc;
}

.circle-row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: 14px;
  width: 100%;
  padding: 14px 12px;
  border: 0;
  border-radius: 20px;
  background: transparent;
  text-align: left;
  cursor: pointer;
}

.circle-row-shell:hover {
  background: #f1f6fb;
}

.circle-row-shell.active {
  background: linear-gradient(135deg, #eff5ff 0%, #eefaf5 100%);
  box-shadow: inset 0 0 0 1px rgba(170, 198, 228, 0.92);
}

.circle-detail-button {
  align-self: stretch;
}

.circle-row:hover,
.circle-row-shell.active .circle-row:hover {
  background: transparent;
}

.circle-avatar {
  width: 42px;
  height: 42px;
  background: linear-gradient(135deg, #dce9ff 0%, #d9f9ef 100%);
  color: #16355c;
  font-weight: 700;
}

.circle-copy {
  display: grid;
  gap: 8px;
  min-width: 0;
}

.restore-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 12px;
  padding: 14px 16px;
  border-radius: 20px;
  background: #f7fafc;
}

.restore-copy {
  display: grid;
  gap: 8px;
  min-width: 0;
}

.row-head,
.row-tags,
.row-meta,
.transport-meta,
.footer-actions,
.empty-tags,
.restore-actions {
  display: flex;
  align-items: center;
}

.row-head {
  justify-content: space-between;
  gap: 12px;
}

.row-tags,
.row-meta,
.transport-meta,
.footer-actions {
  gap: 8px;
  flex-wrap: wrap;
}

.circle-copy strong,
.circle-copy p,
.restore-copy strong,
.restore-copy p {
  margin: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.circle-copy p,
.restore-copy p,
.row-meta,
.transport-meta {
  color: #6d809a;
}

.empty-state-card strong,
.empty-state-card p {
  margin: 0;
}

.empty-state-card p {
  color: #6b7d97;
  line-height: 1.6;
}

.transport-meta {
  min-width: 0;
}

.transport-meta span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mode-grid {
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px;
}

.mode-card {
  display: grid;
  gap: 10px;
  justify-items: start;
  padding: 18px 16px;
  border: 1px solid rgba(208, 218, 228, 0.95);
  border-radius: 22px;
  background: #fbfdff;
  text-align: left;
  cursor: pointer;
}

.mode-card.active {
  border-color: rgba(86, 136, 196, 0.82);
  background: linear-gradient(180deg, #f4f8ff 0%, #f5fbf8 100%);
}

.mode-card i {
  color: #5a81b5;
  font-size: 1.05rem;
}

.mode-card span {
  color: #6d809a;
  line-height: 1.55;
}

.form-grid {
  gap: 12px;
}

.form-field {
  display: grid;
  gap: 8px;
}

.form-field span {
  color: #667a97;
  font-size: 0.86rem;
}

.hint-card {
  display: grid;
  gap: 8px;
  padding: 16px 18px;
  border-radius: 20px;
  background: #f8fbfd;
}

.hint-card p {
  color: #6d809a;
  line-height: 1.65;
}

.footer-actions {
  justify-content: flex-end;
}

@media (max-width: 920px) {
  .mode-grid {
    grid-template-columns: 1fr;
  }
}
</style>
