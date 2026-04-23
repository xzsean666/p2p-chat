<script setup lang="ts">
import { computed, ref, watch } from "vue";
import Avatar from "primevue/avatar";
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import Tag from "primevue/tag";
import Textarea from "primevue/textarea";
import OverlayPageShell from "./OverlayPageShell.vue";
import { deriveCircleRuntimeRetryAction } from "../services/transportDiagnostics";
import type {
  CircleItem,
  CircleTransportDiagnostic,
  DiscoveredPeer,
  SessionSyncItem,
  TransportActivityItem,
  TransportCircleAction,
  TransportEngineKind,
  TransportRuntimeSession,
} from "../types/chat";

const props = defineProps<{
  circle: CircleItem | null;
  isActive: boolean;
  canRemove: boolean;
  transportDiagnostic: CircleTransportDiagnostic | null;
  discoveredPeers: DiscoveredPeer[];
  sessionSyncItems: SessionSyncItem[];
  transportActivities: TransportActivityItem[];
  runtimeSessions: TransportRuntimeSession[];
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

const retryRuntimeAction = computed<TransportCircleAction | null>(() => {
  if (!props.circle) {
    return null;
  }

  return deriveCircleRuntimeRetryAction(props.circle, props.runtimeSessions);
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
  return engine === "nativePreview"
    ? "Experimental native preview"
    : engine === "mock"
      ? "Mock fallback"
      : "unknown";
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

function runtimeTone(state: TransportRuntimeSession["state"]) {
  if (state === "active") {
    return "success";
  }

  if (state === "starting") {
    return "warn";
  }

  return "secondary";
}

function runtimeQueueTone(state: TransportRuntimeSession["queueState"]) {
  if (state === "backoff") {
    return "warn";
  }

  if (state === "queued") {
    return "info";
  }

  return "secondary";
}

function runtimeFailureCopy(session: TransportRuntimeSession) {
  if (!session.lastFailureReason) {
    return "";
  }

  return session.lastFailureAt
    ? `${session.lastFailureReason} · ${session.lastFailureAt}`
    : session.lastFailureReason;
}

function runtimeAdapterTone(kind: TransportRuntimeSession["adapterKind"]) {
  return kind === "localCommand" ? "info" : "secondary";
}

function runtimeLaunchTone(status: TransportRuntimeSession["launchStatus"]) {
  if (status === "ready" || status === "embedded") {
    return "info";
  }

  if (status === "missing") {
    return "danger";
  }

  return "warn";
}

function transportHealthLabel(
  health: CircleTransportDiagnostic["health"],
  engine: TransportEngineKind | null,
) {
  if (engine !== "nativePreview") {
    return health;
  }

  return health === "online"
    ? "preview online"
    : health === "degraded"
      ? "preview warming up"
      : "preview offline";
}

function runtimeLaunchLabel(status: TransportRuntimeSession["launchStatus"]) {
  if (status === "ready") {
    return "command located";
  }

  if (status === "embedded") {
    return "embedded preview";
  }

  if (status === "missing") {
    return "command missing";
  }

  return "not verified";
}

function honestTransportCopy(value: string) {
  const replacements = [
    ["Native runtime active", "Native preview runtime active"],
    ["Native runtime warmup", "Native preview warmup"],
    ["Native runtime offline", "Native preview offline"],
    ["Native runtime booted", "Native preview runtime launched"],
    ["Native runtime stopped", "Native preview runtime stopped"],
    ["Native relay checkpoint saved", "Native preview relay checkpoint saved"],
    ["Native session merge committed", "Native preview session merge committed"],
    ["Native peer sweep finished", "Native preview peer sweep finished"],
    ["native runtime active", "native preview runtime active"],
    ["native runtime booting", "native preview runtime booting"],
    ["native runtime idle", "native preview runtime idle"],
    ["native preview engine", "experimental native preview engine"],
  ] as const;

  return replacements.reduce((nextValue, [search, replacement]) => {
    return nextValue.split(search).join(replacement);
  }, value);
}

function runtimeReadinessCopy(session: TransportRuntimeSession) {
  if (session.launchStatus === "ready") {
    return "Command lookup passed, but this relay still runs on an experimental preview transport path.";
  }

  if (session.launchStatus === "embedded") {
    return "Embedded fallback is mounted for preview diagnostics only.";
  }

  if (session.launchStatus === "missing") {
    return "The experimental runtime command is missing on this host.";
  }

  return "The local preview runtime has not been verified on this host.";
}

function runtimeLaunchCopy(session: TransportRuntimeSession) {
  if (!session.launchCommand) {
    return "";
  }

  return [session.launchCommand, ...session.launchArguments].join(" ");
}

function runtimeLastLaunchCopy(session: TransportRuntimeSession) {
  if (!session.lastLaunchResult) {
    return "";
  }

  const pidCopy = session.lastLaunchPid ? ` pid ${session.lastLaunchPid}` : "";
  const timeCopy = session.lastLaunchAt ? ` · ${session.lastLaunchAt}` : "";
  return `${session.lastLaunchResult}${pidCopy}${timeCopy}`;
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
    title="Circle Info"
    subtitle="Relay, activity and circle actions."
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
        <div class="section-title">Transport</div>
        <div class="info-row">
          <strong>Circle Note</strong>
          <p>
            {{ circle.type === "paid" ? "Private relay onboarding shell." : circle.description }}
          </p>
        </div>
        <div v-if="transportDiagnostic" class="info-row">
          <strong>Transport Health</strong>
          <Tag
            :value="transportHealthLabel(transportDiagnostic.health, transportEngine)"
            :severity="transportTone(transportDiagnostic.health)"
            rounded
          />
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
          <p>{{ honestTransportCopy(transportDiagnostic.lastSync) }}</p>
        </div>
        <p class="section-note">
          Preview diagnostics summarize the current relay shell path. They do not certify end-to-end transport readiness.
        </p>
      </section>

      <section class="section-card">
        <div class="section-head">
          <div class="section-title">Runtime Session</div>
          <Button
            v-if="retryRuntimeAction"
            icon="pi pi-refresh"
            label="Retry Runtime"
            text
            severity="secondary"
            :loading="transportBusy"
            :disabled="transportBusy"
            @click="runTransportAction(retryRuntimeAction)"
          />
        </div>
        <div v-if="runtimeSessions.length" class="list-card">
          <div v-for="item in runtimeSessions" :key="item.sessionLabel" class="list-row">
            <div class="list-copy">
              <strong>{{ item.driver }} · boot #{{ item.generation }}</strong>
              <p>{{ item.sessionLabel }} · {{ item.endpoint }}</p>
              <p v-if="runtimeLaunchCopy(item)">{{ item.adapterKind }} adapter · {{ runtimeLaunchCopy(item) }}</p>
              <p v-else>{{ item.adapterKind }} adapter</p>
              <p>{{ runtimeReadinessCopy(item) }}</p>
              <p v-if="item.resolvedLaunchCommand">resolved {{ item.resolvedLaunchCommand }}</p>
              <p v-if="item.launchError" class="failure-copy">{{ item.launchError }}</p>
              <p v-if="runtimeLastLaunchCopy(item)">last launch {{ runtimeLastLaunchCopy(item) }}</p>
              <p>{{ item.desiredState }} · {{ item.recoveryPolicy }} recovery · state {{ item.stateSince }}</p>
              <p>
                {{ item.queueState }} queue ·
                {{ item.restartAttempts }} recovery attempts{{ item.nextRetryIn ? ` · next ${item.nextRetryIn}` : "" }}
              </p>
              <p v-if="item.lastFailureReason" class="failure-copy">{{ runtimeFailureCopy(item) }}</p>
              <p>{{ honestTransportCopy(item.lastEvent) }} · {{ item.lastEventAt }}</p>
            </div>
            <div class="list-tags">
              <Tag :value="item.adapterKind" :severity="runtimeAdapterTone(item.adapterKind)" rounded />
              <Tag :value="runtimeLaunchLabel(item.launchStatus)" :severity="runtimeLaunchTone(item.launchStatus)" rounded />
              <Tag :value="item.desiredState" :severity="item.desiredState === 'running' ? 'success' : 'secondary'" rounded />
              <Tag :value="item.recoveryPolicy" :severity="item.recoveryPolicy === 'auto' ? 'info' : 'secondary'" rounded />
              <Tag :value="item.queueState" :severity="runtimeQueueTone(item.queueState)" rounded />
              <Tag v-if="item.lastFailureReason" value="failure recorded" severity="danger" rounded />
              <Tag :value="`Boot ${item.generation}`" severity="contrast" rounded />
              <Tag :value="item.state" :severity="runtimeTone(item.state)" rounded />
            </div>
          </div>
        </div>
        <div v-else class="info-row">
          <strong>Runtime</strong>
          <p>No runtime session mounted for this relay.</p>
        </div>
      </section>

      <section class="section-card">
        <div class="section-title">Runtime Activity</div>
        <div v-if="transportActivities.length" class="list-card">
          <div v-for="item in transportActivities" :key="item.id" class="list-row">
            <div class="list-copy">
              <strong>{{ honestTransportCopy(item.title) }}</strong>
              <p>{{ honestTransportCopy(item.detail) }}</p>
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
  gap: 12px;
  padding-top: 8px;
}

.hero-card {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  gap: 12px;
  align-items: center;
  padding: 8px 0 14px;
  border-bottom: 1px solid #e4e9ef;
}

.circle-avatar {
  width: 56px;
  height: 56px;
  background: #eef3f8;
  color: #274c74;
  font-weight: 700;
  font-size: 1.1rem;
}

.hero-copy,
.field {
  display: grid;
  gap: 6px;
}

.section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
}

.hero-copy h3,
.hero-copy p,
.section-title,
.field span,
.copy-feedback,
.missing-state p {
  margin: 0;
}

.failure-copy {
  color: #ad5c2d;
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
  gap: 0;
}

.list-row {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  padding: 14px 0;
  border-top: 1px solid #e7ebf1;
  background: transparent;
}

.list-copy {
  gap: 5px;
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
  gap: 8px;
}

.stat-card {
  display: grid;
  gap: 4px;
  padding: 14px 12px;
  border-radius: 16px;
  background: #ffffff;
  border: 1px solid #e4e9ef;
  text-align: center;
}

.stat-card strong {
  font-size: 1.05rem;
}

.stat-card span,
.field span,
.info-row p {
  color: #6d809a;
}

.section-card {
  gap: 10px;
  padding: 16px;
  border-radius: 20px;
  background: #ffffff;
  border: 1px solid #e4e9ef;
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
  padding: 14px 0;
  border-top: 1px solid #e7ebf1;
  background: transparent;
}

.info-row p {
  margin: 0;
  text-align: right;
  line-height: 1.55;
}

.field + .field,
.field + .info-row,
.info-row + .info-row,
.section-head + .list-card,
.section-head + .info-row,
.section-title + .info-row {
  margin-top: 0;
}

.field:first-of-type,
.info-row:first-of-type,
.list-row:first-of-type {
  border-top: 0;
}

.section-note {
  margin: 0;
  color: #73849a;
  font-size: 0.84rem;
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
