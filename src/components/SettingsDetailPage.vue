<script setup lang="ts">
import { computed, ref, watch } from "vue";
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import Tag from "primevue/tag";
import ToggleSwitch from "primevue/toggleswitch";
import OverlayPageShell from "./OverlayPageShell.vue";
import { loadAuthRuntimeClientUri } from "../services/chatShell";
import type {
  AdvancedPreferences,
  AuthRuntimeBindingSummary,
  AuthRuntimeClientUriSummary,
  AuthRuntimeSummary,
  AuthSessionSummary,
  AppPreferences,
  CircleTransportDiagnostic,
  CircleItem,
  NotificationPreferences,
  RestorableCircleEntry,
  SettingPageId,
  TransportActivityItem,
  TransportRuntimeSession,
  TransportSnapshot,
  UpdateAuthRuntimeInput,
} from "../types/chat";

const props = defineProps<{
  settingId: SettingPageId;
  phase?: string;
  version: string;
  authSession: AuthSessionSummary | null;
  authRuntime: AuthRuntimeSummary | null;
  authRuntimeBinding: AuthRuntimeBindingSummary | null;
  activeCircle: CircleItem | null;
  circlesCount: number;
  restorableCircles: RestorableCircleEntry[];
  sessionCount: number;
  preferences: AppPreferences;
  notifications: NotificationPreferences;
  advanced: AdvancedPreferences;
  transportSnapshot: TransportSnapshot | null;
  activeTransportDiagnostic: CircleTransportDiagnostic | null;
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "open-circle-directory"): void;
  (event: "open-join-circle"): void;
  (event: "update-preferences", patch: Partial<AppPreferences>): void;
  (event: "update-notifications", patch: Partial<NotificationPreferences>): void;
  (event: "update-advanced", patch: Partial<AdvancedPreferences>): void;
  (event: "update-auth-runtime", input: UpdateAuthRuntimeInput): void;
  (event: "sync-auth-runtime"): void;
  (event: "restore-circle", entry: RestorableCircleEntry): void;
  (event: "forget-restorable-circle", relay: string): void;
}>();

const copyFeedback = ref("");
const authRuntimeErrorDraft = ref("");
const authRuntimeClientUri = ref<AuthRuntimeClientUriSummary | null>(null);
const authRuntimeClientUriError = ref("");
const authRuntimeClientUriLoading = ref(false);
let authRuntimeClientUriRequestSerial = 0;

const transportMetrics = computed(() => {
  if (!props.transportSnapshot) {
    return null;
  }

  const pendingSyncCount = props.transportSnapshot.sessionSync.filter((item) => item.state === "pending").length;
  const conflictSyncCount = props.transportSnapshot.sessionSync.filter((item) => item.state === "conflict").length;

  return {
    discoveredPeers: props.transportSnapshot.peers.length,
    syncJobs: props.transportSnapshot.sessionSync.length,
    pendingSyncCount,
    conflictSyncCount,
    activityCount: props.transportSnapshot.activities.length,
    runtimeSessionCount: props.transportSnapshot.runtimeSessions.length,
  };
});

const recentTransportActivities = computed(() => {
  return props.transportSnapshot?.activities.slice(0, 4) ?? [];
});

const runtimeSessions = computed(() => {
  return props.transportSnapshot?.runtimeSessions ?? [];
});

const runtimeSummary = computed(() => {
  if (!runtimeSessions.value.length) {
    return null;
  }

  const drivers = new Map<string, number>();
  let activeCount = 0;
  let startingCount = 0;
  let inactiveCount = 0;
  let desiredRunningCount = 0;
  let autoRecoveryCount = 0;
  let queuedCount = 0;
  let backoffCount = 0;
  let failureCount = 0;

  for (const session of runtimeSessions.value) {
    drivers.set(session.driver, (drivers.get(session.driver) ?? 0) + 1);
    if (session.desiredState === "running") {
      desiredRunningCount += 1;
    }
    if (session.recoveryPolicy === "auto") {
      autoRecoveryCount += 1;
    }
    if (session.queueState === "queued") {
      queuedCount += 1;
    } else if (session.queueState === "backoff") {
      backoffCount += 1;
    }
    if (session.lastFailureReason) {
      failureCount += 1;
    }

    if (session.state === "active") {
      activeCount += 1;
    } else if (session.state === "starting") {
      startingCount += 1;
    } else {
      inactiveCount += 1;
    }
  }

  return {
    activeCount,
    startingCount,
    inactiveCount,
    desiredRunningCount,
    autoRecoveryCount,
    queuedCount,
    backoffCount,
    failureCount,
    drivers: [...drivers.entries()]
      .map(([driver, count]) => ({ driver, count }))
      .sort((left, right) => right.count - left.count),
  };
});

const visibleRuntimeSessions = computed(() => {
  return runtimeSessions.value.slice(0, 4);
});

const restorableCircleCount = computed(() => {
  return props.restorableCircles.length;
});

const activeRuntimeSession = computed(() => {
  const focusCircleId = props.activeCircle?.id ?? props.transportSnapshot?.activeCircleId;
  return (
    runtimeSessions.value.find((session) => session.circleId === focusCircleId) ??
    runtimeSessions.value[0] ??
    null
  );
});

const latestTransportActivity = computed(() => {
  return recentTransportActivities.value[0] ?? null;
});

const canUpdateAuthRuntime = computed(() => {
  return !!props.authSession && props.authSession.loginMethod !== "quickStart";
});

const canSyncAuthRuntime = computed(() => {
  return !!props.authSession && props.authSession.loginMethod !== "quickStart";
});

const supportsAuthRuntimeClientUri = computed(() => {
  return props.settingId === "about" && (
    props.authSession?.access.kind === "bunker" ||
    props.authSession?.access.kind === "nostrConnect"
  );
});

const authRuntimeSyncLabel = computed(() => {
  const accessKind = props.authRuntime?.accessKind ?? props.authSession?.access.kind;
  if (accessKind === "bunker" || accessKind === "nostrConnect") {
    if (props.authRuntime?.state === "pending") {
      return "Retry Handshake";
    }

    if (props.authRuntime?.state === "failed") {
      return "Retry Runtime Sync";
    }

    return "Refresh Signer Runtime";
  }

  return "Refresh Runtime";
});

const metadata = computed(() => {
  switch (props.settingId) {
    case "preferences":
      return {
        title: "Preferences",
        subtitle: "Theme, language and text density for the desktop shell.",
      };
    case "notifications":
      return {
        title: "Notifications",
        subtitle: "Choose how circle activity reaches the desktop.",
      };
    case "advanced":
      return {
        title: "Advanced Settings",
        subtitle: "Transport, diagnostics and extra chat inspection options.",
      };
    case "restore":
      return {
        title: "Restore Circle Access",
        subtitle: "Reconnect private relay access and saved circle entry state.",
      };
    default:
      return {
        title: "About XChat",
        subtitle: "Build information, relay context and desktop shell status.",
      };
  }
});

async function copyValue(label: string, value: string) {
  try {
    await navigator.clipboard.writeText(value);
    copyFeedback.value = `${label} copied`;
  } catch {
    copyFeedback.value = `Clipboard unavailable for ${label.toLowerCase()}`;
  }

  window.setTimeout(() => {
    copyFeedback.value = "";
  }, 1800);
}

function healthTone(health: CircleTransportDiagnostic["health"]) {
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

function runtimeTone(state: TransportRuntimeSession["state"]) {
  if (state === "active") {
    return "success";
  }

  if (state === "starting") {
    return "info";
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
    return "success";
  }

  if (status === "missing") {
    return "danger";
  }

  return "warn";
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

function typeLabel(type: CircleItem["type"]) {
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

function typeTone(type: CircleItem["type"]) {
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

function archivedAtCopy(value: string) {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }

  return parsed.toLocaleString();
}

function emitAuthRuntimeUpdate(state: UpdateAuthRuntimeInput["state"]) {
  if (!props.authSession) {
    return;
  }

  emit("update-auth-runtime", {
    state,
    error:
      state === "connected" || state === "localProfile"
        ? undefined
        : authRuntimeErrorDraft.value.trim() || undefined,
  });
}

watch(
  () => props.authRuntime?.error ?? "",
  (value) => {
    authRuntimeErrorDraft.value = value;
  },
  { immediate: true },
);

watch(
  () => [
    supportsAuthRuntimeClientUri.value,
    props.authSession?.loggedInAt ?? "",
    props.authRuntimeBinding?.updatedAt ?? "",
  ],
  async ([enabled]) => {
    authRuntimeClientUriRequestSerial += 1;
    const requestSerial = authRuntimeClientUriRequestSerial;

    if (!enabled) {
      authRuntimeClientUri.value = null;
      authRuntimeClientUriError.value = "";
      authRuntimeClientUriLoading.value = false;
      return;
    }

    authRuntimeClientUriLoading.value = true;
    authRuntimeClientUriError.value = "";

    try {
      const summary = await loadAuthRuntimeClientUri();
      if (requestSerial !== authRuntimeClientUriRequestSerial) {
        return;
      }

      authRuntimeClientUri.value = summary;
      if (!summary) {
        authRuntimeClientUriError.value = "Desktop native client URI is unavailable for this session.";
      }
    } catch (error) {
      if (requestSerial !== authRuntimeClientUriRequestSerial) {
        return;
      }

      authRuntimeClientUri.value = null;
      authRuntimeClientUriError.value =
        error instanceof Error && error.message.trim()
          ? error.message.trim()
          : "Desktop native client URI could not be generated.";
    } finally {
      if (requestSerial === authRuntimeClientUriRequestSerial) {
        authRuntimeClientUriLoading.value = false;
      }
    }
  },
  { immediate: true },
);
</script>

<template>
  <OverlayPageShell
    :title="metadata.title"
    :subtitle="metadata.subtitle"
    @close="emit('close')"
  >
    <div class="settings-page">
      <template v-if="settingId === 'preferences'">
        <section class="section-card">
          <div class="section-title">Theme</div>
          <div class="option-row">
            <button
              type="button"
              :class="['option-chip', { active: preferences.theme === 'light' }]"
              @click="emit('update-preferences', { theme: 'light' })"
            >
              Morning
            </button>
            <button
              type="button"
              :class="['option-chip', { active: preferences.theme === 'system' }]"
              @click="emit('update-preferences', { theme: 'system' })"
            >
              System
            </button>
            <button
              type="button"
              :class="['option-chip', { active: preferences.theme === 'ink' }]"
              @click="emit('update-preferences', { theme: 'ink' })"
            >
              Ink
            </button>
          </div>
        </section>

        <section class="section-card">
          <div class="section-title">Language</div>
          <div class="option-row">
            <button
              type="button"
              :class="['option-chip', { active: preferences.language === 'en' }]"
              @click="emit('update-preferences', { language: 'en' })"
            >
              English
            </button>
            <button
              type="button"
              :class="['option-chip', { active: preferences.language === 'zh-CN' }]"
              @click="emit('update-preferences', { language: 'zh-CN' })"
            >
              简体中文
            </button>
            <button
              type="button"
              :class="['option-chip', { active: preferences.language === 'system' }]"
              @click="emit('update-preferences', { language: 'system' })"
            >
              Follow System
            </button>
          </div>
        </section>

        <section class="section-card">
          <div class="section-title">Text Size</div>
          <div class="option-row">
            <button
              type="button"
              :class="['option-chip', { active: preferences.textSize === 'compact' }]"
              @click="emit('update-preferences', { textSize: 'compact' })"
            >
              Compact
            </button>
            <button
              type="button"
              :class="['option-chip', { active: preferences.textSize === 'default' }]"
              @click="emit('update-preferences', { textSize: 'default' })"
            >
              Default
            </button>
            <button
              type="button"
              :class="['option-chip', { active: preferences.textSize === 'large' }]"
              @click="emit('update-preferences', { textSize: 'large' })"
            >
              Large
            </button>
          </div>
        </section>
      </template>

      <template v-else-if="settingId === 'notifications'">
        <section class="section-card">
          <div class="toggle-list">
            <div class="toggle-row">
              <div>
                <strong>Allow Send Notifications</strong>
                <p>Surface alerts when this desktop client sends activity to other devices.</p>
              </div>
              <ToggleSwitch
                :model-value="notifications.allowSend"
                @update:model-value="emit('update-notifications', { allowSend: !!$event })"
              />
            </div>

            <div class="toggle-row">
              <div>
                <strong>Allow Receive Notifications</strong>
                <p>Show desktop notifications for active chats and circle events.</p>
              </div>
              <ToggleSwitch
                :model-value="notifications.allowReceive"
                @update:model-value="emit('update-notifications', { allowReceive: !!$event })"
              />
            </div>

            <div class="toggle-row">
              <div>
                <strong>Unread Badge</strong>
                <p>Keep badge counts visible in the shell and task switcher.</p>
              </div>
              <ToggleSwitch
                :model-value="notifications.showBadge"
                @update:model-value="emit('update-notifications', { showBadge: !!$event })"
              />
            </div>

            <div class="toggle-row">
              <div>
                <strong>Archive Summary</strong>
                <p>Include archived chat totals in weekly desktop summaries.</p>
              </div>
              <ToggleSwitch
                :model-value="notifications.archiveSummary"
                @update:model-value="emit('update-notifications', { archiveSummary: !!$event })"
              />
            </div>

            <div class="toggle-row">
              <div>
                <strong>Mentions Only</strong>
                <p>Reduce notification noise and highlight only direct mentions.</p>
              </div>
              <ToggleSwitch
                :model-value="notifications.mentionsOnly"
                @update:model-value="emit('update-notifications', { mentionsOnly: !!$event })"
              />
            </div>
          </div>
        </section>
      </template>

      <template v-else-if="settingId === 'advanced'">
        <section class="section-card">
          <div class="toggle-list">
            <div class="toggle-row">
              <div>
                <strong>Show Message Info</strong>
                <p>Expose delivery and relay metadata in message detail flows.</p>
              </div>
              <ToggleSwitch
                :model-value="advanced.showMessageInfo"
                @update:model-value="emit('update-advanced', { showMessageInfo: !!$event })"
              />
            </div>

            <div class="toggle-row">
              <div>
                <strong>Use Tor Network</strong>
                <p>Route future transport work through a privacy-preserving relay path.</p>
              </div>
              <ToggleSwitch
                :model-value="advanced.useTorNetwork"
                @update:model-value="emit('update-advanced', { useTorNetwork: !!$event })"
              />
            </div>

            <div class="toggle-row">
              <div>
                <strong>Relay Diagnostics</strong>
                <p>Keep extra relay timing and connection diagnostics visible in the shell.</p>
              </div>
              <ToggleSwitch
                :model-value="advanced.relayDiagnostics"
                @update:model-value="emit('update-advanced', { relayDiagnostics: !!$event })"
              />
            </div>

            <div class="toggle-row">
              <div>
                <strong>Experimental Transport</strong>
                <p>Prepare the UI for upcoming P2P transport experiments and debug hooks.</p>
              </div>
              <ToggleSwitch
                :model-value="advanced.experimentalTransport"
                @update:model-value="emit('update-advanced', { experimentalTransport: !!$event })"
              />
            </div>
          </div>
        </section>

        <section class="section-card">
          <div class="section-title">Media Upload Backend</div>
          <div class="option-row">
            <button
              type="button"
              :class="['option-chip', { active: advanced.mediaUploadDriver === 'auto' }]"
              @click="emit('update-advanced', { mediaUploadDriver: 'auto' })"
            >
              Auto
            </button>
            <button
              type="button"
              :class="['option-chip', { active: advanced.mediaUploadDriver === 'local' }]"
              @click="emit('update-advanced', { mediaUploadDriver: 'local' })"
            >
              Local
            </button>
            <button
              type="button"
              :class="['option-chip', { active: advanced.mediaUploadDriver === 'filedrop' }]"
              @click="emit('update-advanced', { mediaUploadDriver: 'filedrop' })"
            >
              Filedrop
            </button>
            <button
              type="button"
              :class="['option-chip', { active: advanced.mediaUploadDriver === 'nip96' }]"
              @click="emit('update-advanced', { mediaUploadDriver: 'nip96' })"
            >
              NIP-96
            </button>
            <button
              type="button"
              :class="['option-chip', { active: advanced.mediaUploadDriver === 'blossom' }]"
              @click="emit('update-advanced', { mediaUploadDriver: 'blossom' })"
            >
              Blossom
            </button>
            <button
              type="button"
              :class="['option-chip', { active: advanced.mediaUploadDriver === 'minio' }]"
              @click="emit('update-advanced', { mediaUploadDriver: 'minio' })"
            >
              MinIO
            </button>
          </div>
          <p class="section-note">
            `Auto` keeps the existing desktop fallback order: persisted config if present, then env, then the
            loopback `chat-media` file server. `Local` always emits local preview URLs. `Filedrop` posts to the
            persisted multipart endpoint below. `NIP-96` accepts either a base origin for well-known discovery
            or a direct upload API URL. `Blossom` signs a native authenticated `PUT /upload` request against
            the configured server. `MinIO` uses the same endpoint field here, while access key / secret / bucket
            still come from desktop env for now.
          </p>
          <div class="field-stack">
            <strong class="field-label">Upload Endpoint</strong>
            <InputText
              :model-value="advanced.mediaUploadEndpoint"
              :disabled="
                advanced.mediaUploadDriver !== 'filedrop' &&
                advanced.mediaUploadDriver !== 'nip96' &&
                advanced.mediaUploadDriver !== 'blossom' &&
                advanced.mediaUploadDriver !== 'minio'
              "
              placeholder="https://files.example.com"
              @update:model-value="
                emit('update-advanced', {
                  mediaUploadEndpoint: typeof $event === 'string' ? $event : '',
                })
              "
            />
            <p class="field-help">
              For `Filedrop`, enter a base origin like `https://filedrop.example.com` or a full upload path like
              `https://cdn.example.com/upload`. For `NIP-96`, enter either a server origin like
              `https://nostr.build` or a direct `api_url`. For `Blossom`, enter a server origin like
              `https://nosto.re` or a direct upload URL ending in `/upload`. For `MinIO`, enter the S3-compatible
              origin, then provide credentials through `P2P_CHAT_MEDIA_UPLOAD_MINIO_ACCESS_KEY`,
              `P2P_CHAT_MEDIA_UPLOAD_MINIO_SECRET_KEY` and `P2P_CHAT_MEDIA_UPLOAD_MINIO_BUCKET`
              in the desktop environment.
            </p>
          </div>
        </section>

        <section v-if="transportSnapshot" class="section-card">
          <div class="section-title">Transport Snapshot</div>
          <div class="info-row">
            <strong>Overall Status</strong>
            <Tag :value="transportSnapshot.status" :severity="healthTone(transportSnapshot.status)" rounded />
          </div>
          <div class="info-row">
            <strong>Transport Engine</strong>
            <p>{{ transportSnapshot.engine }}</p>
          </div>
          <div class="info-row">
            <strong>Connected Relays</strong>
            <p>{{ transportSnapshot.connectedRelays }} / {{ transportSnapshot.relayCount }}</p>
          </div>
          <div class="info-row">
            <strong>Queued Messages</strong>
            <p>{{ transportSnapshot.queuedMessages }}</p>
          </div>
          <div v-if="transportMetrics" class="info-row">
            <strong>Discovered Peers</strong>
            <p>{{ transportMetrics.discoveredPeers }}</p>
          </div>
          <div v-if="transportMetrics" class="info-row">
            <strong>Session Sync</strong>
            <p>
              {{ transportMetrics.syncJobs }} jobs ·
              {{ transportMetrics.pendingSyncCount }} pending ·
              {{ transportMetrics.conflictSyncCount }} conflicts
            </p>
          </div>
          <div v-if="transportMetrics" class="info-row">
            <strong>Activity Entries</strong>
            <p>{{ transportMetrics.activityCount }}</p>
          </div>
          <div v-if="transportMetrics" class="info-row">
            <strong>Runtime Sessions</strong>
            <p>{{ transportMetrics.runtimeSessionCount }}</p>
          </div>
          <div class="tag-row">
            <Tag
              v-if="transportSnapshot.capabilities.supportsMesh"
              value="Mesh Ready"
              severity="secondary"
              rounded
            />
            <Tag
              v-if="transportSnapshot.capabilities.supportsPaidRelays"
              value="Paid Relays"
              severity="warn"
              rounded
            />
            <Tag
              :value="transportSnapshot.capabilities.supportsTor ? 'Tor Enabled' : 'Tor Off'"
              :severity="transportSnapshot.capabilities.supportsTor ? 'contrast' : 'secondary'"
              rounded
            />
            <Tag
              :value="transportSnapshot.capabilities.experimentalEnabled ? 'Experimental On' : 'Experimental Off'"
              :severity="transportSnapshot.capabilities.experimentalEnabled ? 'info' : 'secondary'"
              rounded
            />
          </div>
        </section>

        <section v-if="recentTransportActivities.length" class="section-card">
          <div class="section-title">Recent Runtime Activity</div>
          <div class="list-card">
            <div v-for="item in recentTransportActivities" :key="item.id" class="list-row">
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
        </section>

        <section v-if="transportSnapshot" class="section-card">
          <div class="section-title">Local Runtime</div>
          <template v-if="runtimeSummary">
            <div class="info-row">
              <strong>Driver Mix</strong>
              <p>{{ runtimeSummary.drivers.map((item) => `${item.driver} x${item.count}`).join(" · ") }}</p>
            </div>
            <div class="info-row">
              <strong>Session States</strong>
              <p>
                {{ runtimeSummary.activeCount }} active ·
                {{ runtimeSummary.startingCount }} starting ·
                {{ runtimeSummary.inactiveCount }} inactive
              </p>
            </div>
            <div class="info-row">
              <strong>Desired Runtime</strong>
              <p>
                {{ runtimeSummary.desiredRunningCount }} running desired ·
                {{ runtimeSummary.autoRecoveryCount }} auto recovery
              </p>
            </div>
            <div class="info-row">
              <strong>Recovery Queue</strong>
              <p>
                {{ runtimeSummary.queuedCount }} queued ·
                {{ runtimeSummary.backoffCount }} backing off
              </p>
            </div>
            <div class="info-row">
              <strong>Failure Records</strong>
              <p>{{ runtimeSummary.failureCount }} runtime failures recorded</p>
            </div>
            <div class="list-card">
              <div v-for="item in visibleRuntimeSessions" :key="item.sessionLabel" class="list-row">
                <div class="list-copy">
                  <strong>{{ item.sessionLabel }}</strong>
                  <p>{{ item.endpoint }} · boot #{{ item.generation }} · state {{ item.stateSince }}</p>
                  <p v-if="runtimeLaunchCopy(item)">{{ item.adapterKind }} adapter · {{ runtimeLaunchCopy(item) }}</p>
                  <p v-else>{{ item.adapterKind }} adapter</p>
                  <p v-if="item.resolvedLaunchCommand">resolved {{ item.resolvedLaunchCommand }}</p>
                  <p v-if="item.launchError" class="failure-copy">{{ item.launchError }}</p>
                  <p v-if="runtimeLastLaunchCopy(item)">last launch {{ runtimeLastLaunchCopy(item) }}</p>
                  <p>
                    {{ item.queueState }} queue ·
                    {{ item.restartAttempts }} recovery attempts{{ item.nextRetryIn ? ` · next ${item.nextRetryIn}` : "" }}
                  </p>
                  <p v-if="item.lastFailureReason" class="failure-copy">{{ runtimeFailureCopy(item) }}</p>
                  <p>{{ item.lastEvent }} · {{ item.lastEventAt }}</p>
                </div>
                <div class="list-tags">
                  <Tag :value="item.adapterKind" :severity="runtimeAdapterTone(item.adapterKind)" rounded />
                  <Tag :value="item.launchStatus" :severity="runtimeLaunchTone(item.launchStatus)" rounded />
                  <Tag :value="item.driver" severity="secondary" rounded />
                  <Tag :value="item.desiredState" :severity="item.desiredState === 'running' ? 'success' : 'secondary'" rounded />
                  <Tag :value="item.recoveryPolicy" :severity="item.recoveryPolicy === 'auto' ? 'info' : 'secondary'" rounded />
                  <Tag :value="item.queueState" :severity="runtimeQueueTone(item.queueState)" rounded />
                  <Tag v-if="item.lastFailureReason" value="failure recorded" severity="danger" rounded />
                  <Tag :value="`Boot ${item.generation}`" severity="contrast" rounded />
                  <Tag :value="item.state" :severity="runtimeTone(item.state)" rounded />
                </div>
              </div>
            </div>
          </template>
          <p v-else class="empty-state">No local runtime session mounted yet.</p>
        </section>
      </template>

      <template v-else-if="settingId === 'restore'">
        <section class="hero-card">
          <div class="hero-copy">
            <h3>Restore Circle Access</h3>
            <p>Removed circles are archived into a local restore catalog. You can bring them back with the original relay, type and description intact.</p>
          </div>
          <div class="hero-tags">
            <Tag value="Circle Restore" severity="warn" rounded />
            <Tag :value="`${restorableCircleCount} saved`" severity="contrast" rounded />
          </div>
        </section>

        <section class="section-card">
          <div class="info-row">
            <strong>What Gets Restored</strong>
            <p>Relay entries, active circle selection and shell-level onboarding context.</p>
          </div>
          <div class="info-row">
            <strong>Current Circle</strong>
            <p>{{ activeCircle?.name ?? "No active circle" }}</p>
          </div>
          <div class="info-row">
            <strong>Saved Circles</strong>
            <p>{{ circlesCount }}</p>
          </div>
          <div class="info-row">
            <strong>Restorable Catalog</strong>
            <p>{{ restorableCircleCount }}</p>
          </div>
        </section>

        <section class="section-card">
          <div class="section-title">Saved Restore Entries</div>
          <div v-if="restorableCircles.length" class="list-card">
            <div v-for="entry in restorableCircles" :key="entry.relay" class="list-row restore-row">
              <div class="list-copy">
                <strong>{{ entry.name }}</strong>
                <p>{{ entry.relay }}</p>
                <p>{{ entry.description || "No description archived for this circle." }}</p>
                <p>Archived {{ archivedAtCopy(entry.archivedAt) }}</p>
              </div>
              <div class="restore-actions">
                <Tag :value="typeLabel(entry.type)" :severity="typeTone(entry.type)" rounded />
                <Button
                  icon="pi pi-refresh"
                  label="Restore"
                  severity="contrast"
                  @click="emit('restore-circle', entry)"
                />
                <Button
                  icon="pi pi-trash"
                  label="Forget"
                  text
                  severity="secondary"
                  @click="emit('forget-restorable-circle', entry.relay)"
                />
              </div>
            </div>
          </div>
          <p v-else class="empty-state">
            No archived circles yet. When you remove a circle from the shell, it will appear here for later restore.
          </p>
        </section>
      </template>

      <template v-else>
        <section class="hero-card">
          <div class="hero-copy">
            <h3>P2P Chat Desktop</h3>
            <p>Rust + Tauri + Vue shell rebuilding the original mobile interaction model for desktop.</p>
          </div>
          <div class="hero-tags">
            <Tag :value="`v${version}`" severity="contrast" rounded />
            <Tag :value="phase || 'Foundation'" severity="info" rounded />
          </div>
        </section>

        <section class="section-card">
          <div class="info-row">
            <strong>Account Session</strong>
            <p v-if="authSession">
              {{ authSession.loginMethod }} · {{ authSession.circleSelectionMode }}
            </p>
            <p v-else>No authenticated account bootstrapped in the local shell.</p>
          </div>
          <div v-if="authSession" class="info-row">
            <strong>Access Summary</strong>
            <p>{{ authSession.access.kind }} · {{ authSession.access.label }}</p>
          </div>
          <div v-if="authRuntime?.pubkey || authSession?.access.pubkey" class="info-row">
            <strong>Verified Pubkey</strong>
            <p>{{ authRuntime?.pubkey ?? authSession?.access.pubkey }}</p>
          </div>
          <div v-if="authSession" class="info-row">
            <strong>Authenticated At</strong>
            <p>{{ authSession.loggedInAt }}</p>
          </div>
          <div class="info-row">
            <strong>Auth Runtime</strong>
            <p v-if="authRuntime">
              {{ authRuntime.state }} · {{ authRuntime.accessKind }} · {{ authRuntime.label }}
            </p>
            <p v-else>No auth runtime state persisted yet.</p>
          </div>
          <div v-if="authRuntime" class="info-row">
            <strong>Auth Runtime Updated</strong>
            <p>{{ authRuntime.updatedAt }}</p>
          </div>
          <div v-if="authRuntime" class="info-row">
            <strong>Auth Runtime Source</strong>
            <p>{{ authRuntime.persistedInNativeStore ? "native store" : "local fallback" }}</p>
          </div>
          <div
            v-if="
              authRuntime &&
              (authRuntime.accessKind === 'nsec' || authRuntime.accessKind === 'hexKey')
            "
            class="info-row"
          >
            <strong>Local Credential</strong>
            <p>
              {{
                authRuntime.credentialPersistedInNativeStore
                  ? 'native credential store'
                  : 'missing from native credential store'
              }}
            </p>
          </div>
          <div v-if="authRuntime" class="info-row">
            <strong>Send Status</strong>
            <p>{{ authRuntime.canSendMessages ? "ready" : "blocked" }}</p>
          </div>
          <div v-if="authRuntime?.sendBlockedReason" class="info-row">
            <strong>Send Gate Reason</strong>
            <p>{{ authRuntime.sendBlockedReason }}</p>
          </div>
          <div v-if="authRuntime?.error" class="info-row">
            <strong>Auth Runtime Error</strong>
            <p>{{ authRuntime.error }}</p>
          </div>
          <div v-if="authRuntimeBinding" class="info-row">
            <strong>Auth Runtime Binding</strong>
            <p>
              {{ authRuntimeBinding.accessKind }} · {{ authRuntimeBinding.endpoint }} ·
              {{ authRuntimeBinding.persistedInNativeStore ? "native store" : "local fallback" }}
            </p>
          </div>
          <div v-if="authRuntimeBinding?.connectionPubkey" class="info-row">
            <strong>Binding Pubkey</strong>
            <p>{{ authRuntimeBinding.connectionPubkey }}</p>
          </div>
          <div v-if="authRuntimeBinding" class="info-row">
            <strong>Binding Relays</strong>
            <p>{{ authRuntimeBinding.relayCount }}</p>
          </div>
          <div v-if="authRuntimeBinding" class="info-row">
            <strong>Binding Secret</strong>
            <p>{{ authRuntimeBinding.hasSecret ? "present in URI" : "not embedded" }}</p>
          </div>
          <div v-if="authRuntimeBinding?.requestedPermissions?.length" class="info-row">
            <strong>Requested Permissions</strong>
            <p>{{ authRuntimeBinding.requestedPermissions.join(", ") }}</p>
          </div>
          <div v-if="authRuntimeBinding?.clientName" class="info-row">
            <strong>Binding Client Name</strong>
            <p>{{ authRuntimeBinding.clientName }}</p>
          </div>
          <div v-if="authRuntimeBinding" class="info-row">
            <strong>Binding Updated</strong>
            <p>{{ authRuntimeBinding.updatedAt }}</p>
          </div>
          <div v-if="supportsAuthRuntimeClientUri" class="info-row auth-runtime-controls">
            <strong>Standard Client URI</strong>
            <div class="auth-runtime-panel">
              <p class="runtime-note">
                Share this standard `nostrconnect://...?metadata=...` client URI with a signer app that expects the
                current NIP-46 client flow.
              </p>
              <p v-if="authRuntimeClientUriLoading" class="runtime-note">Generating desktop client URI...</p>
              <template v-else-if="authRuntimeClientUri">
                <textarea
                  class="uri-preview"
                  readonly
                  :value="authRuntimeClientUri.uri"
                />
                <p class="runtime-note">
                  {{ authRuntimeClientUri.clientName }} · {{ authRuntimeClientUri.relayCount }} relays
                </p>
                <p v-if="authRuntimeClientUri.relays.length" class="runtime-note">
                  {{ authRuntimeClientUri.relays.join(", ") }}
                </p>
                <div class="auth-runtime-actions">
                  <Button
                    label="Copy Client URI"
                    icon="pi pi-copy"
                    text
                    severity="info"
                    @click="copyValue('Client URI', authRuntimeClientUri.uri)"
                  />
                  <Button
                    label="Copy Client Pubkey"
                    icon="pi pi-key"
                    text
                    severity="secondary"
                    @click="copyValue('Client Pubkey', authRuntimeClientUri.publicKey)"
                  />
                </div>
              </template>
              <p v-else-if="authRuntimeClientUriError" class="runtime-note failure-copy">
                {{ authRuntimeClientUriError }}
              </p>
            </div>
          </div>
          <div v-if="authSession" class="info-row auth-runtime-controls">
            <strong>Auth Runtime Controls</strong>
            <div class="auth-runtime-panel">
              <p v-if="!canUpdateAuthRuntime" class="runtime-note">
                Quick Start stays on `localProfile` and does not require a remote signer handshake.
              </p>
              <template v-else>
                <p class="runtime-note">
                  Re-run native auth runtime sync and signer checks without waiting for the next automatic poll.
                </p>
                <InputText
                  v-model="authRuntimeErrorDraft"
                  placeholder="Optional pending/failure detail"
                />
                <div class="auth-runtime-actions">
                  <Button
                    v-if="canSyncAuthRuntime"
                    :label="authRuntimeSyncLabel"
                    icon="pi pi-refresh"
                    text
                    severity="info"
                    @click="emit('sync-auth-runtime')"
                  />
                  <Button
                    label="Mark Pending"
                    text
                    severity="secondary"
                    @click="emitAuthRuntimeUpdate('pending')"
                  />
                  <Button
                    label="Mark Connected"
                    text
                    severity="success"
                    @click="emitAuthRuntimeUpdate('connected')"
                  />
                  <Button
                    label="Mark Failed"
                    text
                    severity="danger"
                    @click="emitAuthRuntimeUpdate('failed')"
                  />
                </div>
              </template>
            </div>
          </div>
          <div class="info-row">
            <strong>Current Circle</strong>
            <p>{{ activeCircle?.name ?? "No active circle" }}</p>
          </div>
          <div class="info-row">
            <strong>Relay Endpoint</strong>
            <p>{{ activeCircle?.relay ?? "Unavailable" }}</p>
          </div>
          <div class="info-row">
            <strong>Shell Counts</strong>
            <p>{{ circlesCount }} circles · {{ sessionCount }} sessions</p>
          </div>
          <div class="info-row">
            <strong>Storage</strong>
            <p>SQLite in desktop mode, browser fallback to localStorage.</p>
          </div>
          <div v-if="activeTransportDiagnostic" class="info-row">
            <strong>Transport</strong>
            <p>
              {{ activeTransportDiagnostic.protocol }} ·
              {{ activeTransportDiagnostic.peerCount }} peers ·
              {{ activeTransportDiagnostic.lastSync }}
            </p>
          </div>
          <div v-if="transportMetrics" class="info-row">
            <strong>Discovery and Sync</strong>
            <p>{{ transportMetrics.discoveredPeers }} peers · {{ transportMetrics.syncJobs }} sync jobs</p>
          </div>
          <div v-if="runtimeSummary" class="info-row">
            <strong>Runtime Sessions</strong>
            <p>
              {{ runtimeSummary.activeCount }} active ·
              {{ runtimeSummary.startingCount }} starting ·
              {{ runtimeSummary.inactiveCount }} inactive
            </p>
          </div>
          <div v-if="activeRuntimeSession" class="info-row">
            <strong>Runtime Driver</strong>
            <p>{{ activeRuntimeSession.driver }} · {{ activeRuntimeSession.endpoint }}</p>
          </div>
          <div v-if="activeRuntimeSession" class="info-row">
            <strong>Runtime Adapter</strong>
            <p>
              {{ activeRuntimeSession.adapterKind }}
              {{ runtimeLaunchCopy(activeRuntimeSession) ? ` · ${runtimeLaunchCopy(activeRuntimeSession)}` : "" }}
            </p>
          </div>
          <div v-if="activeRuntimeSession" class="info-row">
            <strong>Runtime Launch</strong>
            <p>
              {{ activeRuntimeSession.launchStatus }}
              {{ activeRuntimeSession.resolvedLaunchCommand ? ` · ${activeRuntimeSession.resolvedLaunchCommand}` : "" }}
              {{ activeRuntimeSession.launchError ? ` · ${activeRuntimeSession.launchError}` : "" }}
            </p>
          </div>
          <div v-if="activeRuntimeSession && runtimeLastLaunchCopy(activeRuntimeSession)" class="info-row">
            <strong>Runtime Launch Attempt</strong>
            <p>{{ runtimeLastLaunchCopy(activeRuntimeSession) }}</p>
          </div>
          <div v-if="activeRuntimeSession" class="info-row">
            <strong>Runtime Lifecycle</strong>
            <p>boot #{{ activeRuntimeSession.generation }} · state {{ activeRuntimeSession.stateSince }}</p>
          </div>
          <div v-if="activeRuntimeSession" class="info-row">
            <strong>Runtime Intent</strong>
            <p>{{ activeRuntimeSession.desiredState }} · {{ activeRuntimeSession.recoveryPolicy }} recovery</p>
          </div>
          <div v-if="activeRuntimeSession" class="info-row">
            <strong>Runtime Queue</strong>
            <p>
              {{ activeRuntimeSession.queueState }} ·
              {{ activeRuntimeSession.restartAttempts }} recovery attempts{{ activeRuntimeSession.nextRetryIn ? ` · next ${activeRuntimeSession.nextRetryIn}` : "" }}
            </p>
          </div>
          <div v-if="activeRuntimeSession?.lastFailureReason" class="info-row">
            <strong>Runtime Failure</strong>
            <p>{{ runtimeFailureCopy(activeRuntimeSession) }}</p>
          </div>
          <div v-if="activeRuntimeSession" class="info-row">
            <strong>Runtime Event</strong>
            <p>{{ activeRuntimeSession.lastEvent }} · {{ activeRuntimeSession.lastEventAt }}</p>
          </div>
          <div v-if="latestTransportActivity" class="info-row">
            <strong>Latest Activity</strong>
            <p>{{ latestTransportActivity.title }} · {{ latestTransportActivity.time }}</p>
          </div>
        </section>

        <section class="section-card">
          <div class="section-title">Stack</div>
          <div class="tag-row">
            <Tag value="Rust" severity="secondary" rounded />
            <Tag value="Tauri 2" severity="secondary" rounded />
            <Tag value="Vue 3" severity="secondary" rounded />
            <Tag value="PrimeVue" severity="secondary" rounded />
            <Tag value="TypeScript" severity="secondary" rounded />
          </div>
        </section>
      </template>

      <p v-if="copyFeedback" class="copy-feedback">{{ copyFeedback }}</p>
    </div>

    <template #footer>
      <div class="footer-actions">
        <Button v-if="settingId === 'about'" label="Copy Version" text severity="secondary" @click="copyValue('Version', version)" />
        <Button
          v-if="settingId === 'about' && activeCircle"
          label="Copy Relay"
          text
          severity="secondary"
          @click="copyValue('Relay', activeCircle.relay)"
        />
        <Button
          v-if="settingId === 'restore'"
          icon="pi pi-compass"
          label="Join with Invite"
          text
          severity="secondary"
          @click="emit('open-join-circle')"
        />
        <Button
          v-if="settingId === 'restore'"
          icon="pi pi-compass"
          label="Open Circle Directory"
          text
          severity="secondary"
          @click="emit('open-circle-directory')"
        />
        <Button label="Close" severity="contrast" @click="emit('close')" />
      </div>
    </template>
  </OverlayPageShell>
</template>

<style scoped>
.settings-page,
.section-card,
.toggle-list,
.hero-card,
.list-card,
.list-copy {
  display: grid;
}

.settings-page {
  gap: 18px;
}

.section-card {
  gap: 12px;
}

.list-card {
  gap: 10px;
}

.section-title {
  color: var(--shell-text-muted);
  text-transform: uppercase;
  letter-spacing: 0.14em;
  font-size: 0.72rem;
}

.option-row,
.footer-actions,
.hero-tags,
.tag-row,
.list-tags,
.restore-actions {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.field-stack {
  display: grid;
  gap: 8px;
}

.field-label {
  font-size: 0.85rem;
}

.field-help,
.section-note {
  margin: 0;
  color: var(--shell-text-muted);
  line-height: 1.6;
}

.option-chip {
  padding: 10px 14px;
  border: 1px solid var(--shell-border);
  border-radius: 999px;
  background: var(--shell-surface-strong);
  color: var(--shell-text-default);
  cursor: pointer;
}

.option-chip.active {
  border-color: var(--shell-selected-border);
  background: var(--shell-selected);
  color: var(--shell-text-strong);
}

.toggle-list {
  gap: 10px;
}

.toggle-row,
.info-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 14px 16px;
  border-radius: 20px;
  background: var(--shell-surface-soft);
}

.list-row {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  padding: 14px 16px;
  border-radius: 20px;
  background: var(--shell-surface-soft);
}

.toggle-row strong,
.toggle-row p,
.info-row strong,
.info-row p,
.hero-copy h3,
.hero-copy p,
.list-copy strong,
.list-copy p,
.copy-feedback,
.empty-state {
  margin: 0;
}

.toggle-row div,
.hero-copy,
.list-copy {
  display: grid;
  gap: 6px;
}

.toggle-row p,
.info-row p,
.hero-copy p,
.list-copy p,
.empty-state {
  color: var(--shell-text-muted);
  line-height: 1.6;
}

.failure-copy {
  color: #ad5c2d;
}

.hero-card {
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 16px;
  align-items: start;
  padding: 24px;
  border-radius: 28px;
  background:
    radial-gradient(circle at top left, rgba(106, 168, 255, 0.18), transparent 26%),
    linear-gradient(
      180deg,
      color-mix(in srgb, var(--shell-surface-strong) 94%, rgba(106, 168, 255, 0.08)) 0%,
      color-mix(in srgb, var(--shell-surface-soft) 96%, rgba(111, 214, 176, 0.06)) 100%
    );
}

.copy-feedback {
  color: #2f8c6a;
  font-size: 0.9rem;
}

.auth-runtime-controls {
  align-items: start;
}

.auth-runtime-panel {
  display: grid;
  gap: 10px;
  width: min(100%, 420px);
}

.auth-runtime-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.uri-preview {
  min-height: 108px;
  width: 100%;
  padding: 12px 14px;
  border: 1px solid var(--shell-border);
  border-radius: 16px;
  background: var(--shell-surface-strong);
  color: var(--shell-text-default);
  font: inherit;
  line-height: 1.5;
  resize: vertical;
}

.runtime-note {
  margin: 0;
}

.restore-row {
  align-items: center;
}

@media (max-width: 720px) {
  .toggle-row,
  .info-row,
  .list-row,
  .hero-card {
    grid-template-columns: 1fr;
    align-items: start;
  }

  .toggle-row,
  .info-row,
  .list-row {
    flex-direction: column;
  }

  .footer-actions {
    justify-content: stretch;
  }
}
</style>
