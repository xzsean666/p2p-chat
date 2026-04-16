<script setup lang="ts">
import { computed, ref, watch } from "vue";
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import Tag from "primevue/tag";
import Textarea from "primevue/textarea";
import OverlayPageShell from "./OverlayPageShell.vue";
import type {
  CircleItem,
  CircleTransportDiagnostic,
  DiscoveredPeer,
  SessionSyncItem,
  TransportActivityItem,
  TransportCircleAction,
  TransportEngineKind,
} from "../types/chat";

const props = defineProps<{
  circle: CircleItem | null;
  isActive: boolean;
  canRemove: boolean;
  transportDiagnostic: CircleTransportDiagnostic | null;
  discoveredPeers: DiscoveredPeer[];
  sessionSyncItems: SessionSyncItem[];
  transportActivities: TransportActivityItem[];
  transportEngine: TransportEngineKind | null;
  transportBusy: boolean;
  sessionCount: number;
  directCount: number;
  groupCount: number;
  archivedCount: number;
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "select-circle", circleId: string): void;
  (event: "update-circle", payload: { circleId: string; name: string; description: string }): void;
  (event: "remove-circle", circleId: string): void;
  (event: "transport-action", circleId: string, action: TransportCircleAction): void;
}>();

const draftName = ref("");
const draftDescription = ref("");
const copyFeedback = ref("");

watch(
  () => props.circle,
  (circle) => {
    draftName.value = circle?.name ?? "";
    draftDescription.value = circle?.description ?? "";
  },
  { immediate: true },
);

const isDirty = computed(() => {
  return (
    draftName.value.trim() !== (props.circle?.name ?? "") ||
    draftDescription.value.trim() !== (props.circle?.description ?? "")
  );
});

function typeTone(type: CircleItem["type"]) {
  if (type === "paid") {
    return "warn";
  }

  if (type === "custom") {
    return "contrast";
  }

  if (type === "bitchat") {
    return "secondary";
  }

  return "success";
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

function activityTone(level: TransportActivityItem["level"]) {
  if (level === "success") {
    return "success";
  }

  if (level === "warn") {
    return "warn";
  }

  return "info";
}

function presenceTone(presence: DiscoveredPeer["presence"]) {
  if (presence === "online") {
    return "success";
  }

  if (presence === "idle") {
    return "warn";
  }

  return "secondary";
}

function engineLabel(engine: TransportEngineKind | null) {
  return engine === "nativePreview" ? "nativePreview" : engine === "mock" ? "mock" : "unknown";
}

function syncTone(state: SessionSyncItem["state"]) {
  if (state === "pending") {
    return "warn";
  }

  if (state === "syncing") {
    return "info";
  }

  if (state === "conflict") {
    return "danger";
  }

  return "secondary";
}

async function copyRelay() {
  if (!props.circle) {
    return;
  }

  try {
    await navigator.clipboard.writeText(props.circle.relay);
    copyFeedback.value = "Relay copied";
  } catch {
    copyFeedback.value = "Clipboard unavailable";
  }

  window.setTimeout(() => {
    copyFeedback.value = "";
  }, 1800);
}

function saveChanges() {
  if (!props.circle) {
    return;
  }

  emit("update-circle", {
    circleId: props.circle.id,
    name: draftName.value.trim(),
    description: draftDescription.value.trim(),
  });
}

function runTransportAction(action: TransportCircleAction) {
  if (!props.circle) {
    return;
  }

  emit("transport-action", props.circle.id, action);
}
</script>

<template>
  <OverlayPageShell
    title="Circle Settings"
    subtitle="Relay identity, shell metrics and circle actions."
    @close="emit('close')"
  >
    <div v-if="circle" class="circle-detail-page">
      <section class="hero-card">
        <Avatar :label="circle.name.slice(0, 1)" shape="circle" class="circle-avatar" />
        <div class="hero-copy">
          <h3>{{ circle.name }}</h3>
          <p>{{ circle.relay }}</p>
        </div>
        <div class="hero-tags">
          <Tag :value="circle.type" :severity="typeTone(circle.type)" rounded />
          <Tag :value="circle.status" :severity="statusTone(circle.status)" rounded />
          <Tag v-if="transportEngine" :value="engineLabel(transportEngine)" severity="contrast" rounded />
          <Tag v-if="isActive" value="Active" severity="info" rounded />
        </div>
      </section>

      <section class="stats-grid">
        <div class="stat-card">
          <strong>{{ sessionCount }}</strong>
          <span>Sessions</span>
        </div>
        <div class="stat-card">
          <strong>{{ directCount }}</strong>
          <span>Direct</span>
        </div>
        <div class="stat-card">
          <strong>{{ groupCount }}</strong>
          <span>Groups</span>
        </div>
        <div class="stat-card">
          <strong>{{ archivedCount }}</strong>
          <span>Archived</span>
        </div>
      </section>

      <section class="section-card">
        <div class="section-title">Identity</div>

        <label class="field">
          <span>Name</span>
          <InputText v-model="draftName" placeholder="Circle name" />
        </label>

        <label class="field">
          <span>Description</span>
          <Textarea
            v-model="draftDescription"
            auto-resize
            rows="3"
            placeholder="Describe the role of this relay space"
          />
        </label>

        <div class="info-row">
          <strong>Relay</strong>
          <p>{{ circle.relay }}</p>
        </div>
        <div class="info-row">
          <strong>Latency</strong>
          <p>{{ circle.latency }}</p>
        </div>
      </section>

      <section class="section-card">
        <div class="section-title">Notes</div>
        <div class="info-row">
          <strong>Shell Status</strong>
          <p>
            {{ circle.type === "paid" ? "Private relay onboarding shell." : circle.description }}
          </p>
        </div>
        <div v-if="transportDiagnostic" class="info-row">
          <strong>Transport Health</strong>
          <Tag :value="transportDiagnostic.health" :severity="transportTone(transportDiagnostic.health)" rounded />
        </div>
        <div v-if="transportEngine" class="info-row">
          <strong>Transport Engine</strong>
          <p>{{ engineLabel(transportEngine) }}</p>
        </div>
        <div v-if="transportDiagnostic" class="info-row">
          <strong>Protocol</strong>
          <p>{{ transportDiagnostic.protocol }}</p>
        </div>
        <div v-if="transportDiagnostic" class="info-row">
          <strong>Peers and Queue</strong>
          <p>{{ transportDiagnostic.peerCount }} peers · {{ transportDiagnostic.queuedMessages }} queued</p>
        </div>
        <div v-if="transportDiagnostic" class="info-row">
          <strong>Last Sync</strong>
          <p>{{ transportDiagnostic.lastSync }}</p>
        </div>
      </section>

      <section class="section-card">
        <div class="section-title">Runtime Activity</div>
        <div v-if="transportActivities.length" class="list-card">
          <div v-for="item in transportActivities" :key="item.id" class="list-row">
            <div class="list-copy">
              <strong>{{ item.title }}</strong>
              <p>{{ item.detail }}</p>
            </div>
            <div class="list-tags">
              <Tag :value="item.kind" :severity="activityTone(item.level)" rounded />
              <Tag :value="item.time" severity="secondary" rounded />
            </div>
          </div>
        </div>
        <div v-else class="info-row">
          <strong>Activity</strong>
          <p>No runtime activity recorded for this relay yet.</p>
        </div>
      </section>

      <section class="section-card">
        <div class="section-title">Peer Discovery</div>
        <div v-if="discoveredPeers.length" class="list-card">
          <div v-for="peer in discoveredPeers" :key="peer.contactId" class="list-row">
            <div class="list-copy">
              <strong>{{ peer.name }}</strong>
              <p>{{ peer.handle }} · {{ peer.route }} · {{ peer.sharedSessions }} shared sessions</p>
            </div>
            <div class="list-tags">
              <Tag :value="peer.presence" :severity="presenceTone(peer.presence)" rounded />
              <Tag v-if="peer.blocked" value="Blocked" severity="secondary" rounded />
            </div>
          </div>
        </div>
        <div v-else class="info-row">
          <strong>Peers</strong>
          <p>No peers discovered yet for this relay.</p>
        </div>
      </section>

      <section class="section-card">
        <div class="section-title">Session Sync</div>
        <div v-if="sessionSyncItems.length" class="list-card">
          <div v-for="item in sessionSyncItems" :key="item.sessionId" class="list-row">
            <div class="list-copy">
              <strong>{{ item.sessionName }}</strong>
              <p>{{ item.source }} · {{ item.pendingMessages }} pending · {{ item.lastMerge }}</p>
            </div>
            <div class="list-tags">
              <Tag :value="item.state" :severity="syncTone(item.state)" rounded />
            </div>
          </div>
        </div>
        <div v-else class="info-row">
          <strong>Sync Jobs</strong>
          <p>No session sync records for this relay.</p>
        </div>
      </section>

      <p v-if="copyFeedback" class="copy-feedback">{{ copyFeedback }}</p>
    </div>

    <div v-else class="missing-state">
      <i class="pi pi-globe"></i>
      <p>This circle is no longer available.</p>
    </div>

    <template v-if="circle" #footer>
      <div class="footer-actions">
        <Button
          v-if="!isActive"
          icon="pi pi-check"
          label="Make Active"
          text
          severity="secondary"
          @click="emit('select-circle', circle.id)"
        />
        <Button
          label="Discover Peers"
          severity="secondary"
          text
          :loading="transportBusy"
          :disabled="transportBusy || circle.status === 'closed'"
          @click="runTransportAction('discoverPeers')"
        />
        <Button
          label="Sync Sessions"
          severity="secondary"
          text
          :loading="transportBusy"
          :disabled="transportBusy || circle.status === 'closed'"
          @click="runTransportAction('syncSessions')"
        />
        <Button
          v-if="circle.status !== 'open'"
          :label="circle.status === 'connecting' ? 'Finish Sync' : 'Connect Relay'"
          severity="secondary"
          text
          :loading="transportBusy"
          :disabled="transportBusy"
          @click="runTransportAction(circle.status === 'connecting' ? 'sync' : 'connect')"
        />
        <Button
          v-if="circle.status === 'open'"
          label="Sync Relay"
          severity="secondary"
          text
          :loading="transportBusy"
          :disabled="transportBusy"
          @click="runTransportAction('sync')"
        />
        <Button
          v-if="circle.status !== 'closed'"
          label="Disconnect"
          severity="secondary"
          text
          :disabled="transportBusy"
          @click="runTransportAction('disconnect')"
        />
        <Button label="Copy Relay" text severity="secondary" @click="copyRelay" />
        <Button label="Save Changes" severity="contrast" :disabled="!isDirty" @click="saveChanges" />
        <Button
          icon="pi pi-trash"
          label="Remove Circle"
          severity="danger"
          text
          :disabled="!canRemove"
          @click="emit('remove-circle', circle.id)"
        />
      </div>
    </template>
  </OverlayPageShell>
</template>

<style scoped>
.circle-detail-page,
.section-card,
.stats-grid {
  display: grid;
}

.circle-detail-page {
  gap: 18px;
}

.hero-card {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  gap: 16px;
  align-items: center;
  padding: 24px;
  border-radius: 28px;
  background:
    radial-gradient(circle at top left, rgba(106, 168, 255, 0.18), transparent 26%),
    linear-gradient(180deg, #f7fbfe 0%, #f2f7fb 100%);
}

.circle-avatar {
  width: 58px;
  height: 58px;
  background: linear-gradient(135deg, #dce9ff 0%, #d9f9ef 100%);
  color: #16355c;
  font-weight: 700;
  font-size: 1.2rem;
}

.hero-copy,
.field {
  display: grid;
  gap: 8px;
}

.hero-copy h3,
.hero-copy p,
.section-title,
.field span,
.copy-feedback,
.missing-state p {
  margin: 0;
}

.hero-copy p {
  color: #6d809a;
  word-break: break-all;
}

.hero-tags,
.footer-actions,
.list-tags {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.list-card,
.list-copy {
  display: grid;
}

.list-card {
  gap: 10px;
}

.list-row {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  padding: 14px 16px;
  border-radius: 20px;
  background: #f7fafc;
}

.list-copy {
  gap: 6px;
}

.list-copy strong,
.list-copy p {
  margin: 0;
}

.list-copy p {
  color: #6d809a;
  line-height: 1.6;
}

.stats-grid {
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 10px;
}

.stat-card {
  display: grid;
  gap: 6px;
  padding: 16px;
  border-radius: 20px;
  background: #f7fafc;
  text-align: center;
}

.stat-card strong {
  font-size: 1.2rem;
}

.stat-card span,
.field span,
.info-row p {
  color: #6d809a;
}

.section-card {
  gap: 12px;
}

.section-title {
  color: #6a7d98;
  text-transform: uppercase;
  letter-spacing: 0.14em;
  font-size: 0.72rem;
}

.field {
  gap: 8px;
}

.info-row {
  display: flex;
  align-items: start;
  justify-content: space-between;
  gap: 16px;
  padding: 14px 16px;
  border-radius: 20px;
  background: #f7fafc;
}

.info-row p {
  margin: 0;
  text-align: right;
  line-height: 1.55;
}

.copy-feedback {
  color: #2f8c6a;
  font-size: 0.9rem;
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

@media (max-width: 860px) {
  .hero-card {
    grid-template-columns: 1fr;
    justify-items: start;
  }

  .stats-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .info-row {
    flex-direction: column;
  }

  .info-row p {
    text-align: left;
  }
}

@media (max-width: 640px) {
  .stats-grid {
    grid-template-columns: 1fr;
  }
}
</style>
