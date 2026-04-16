<script setup lang="ts">
import { computed, ref } from "vue";
import Button from "primevue/button";
import Tag from "primevue/tag";
import ToggleSwitch from "primevue/toggleswitch";
import OverlayPageShell from "./OverlayPageShell.vue";
import type {
  AdvancedPreferences,
  CircleTransportDiagnostic,
  AppPreferences,
  CircleItem,
  NotificationPreferences,
  SettingPageId,
  TransportActivityItem,
  TransportSnapshot,
} from "../types/chat";

const props = defineProps<{
  settingId: SettingPageId;
  phase?: string;
  version: string;
  activeCircle: CircleItem | null;
  circlesCount: number;
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
  (event: "update-preferences", patch: Partial<AppPreferences>): void;
  (event: "update-notifications", patch: Partial<NotificationPreferences>): void;
  (event: "update-advanced", patch: Partial<AdvancedPreferences>): void;
}>();

const copyFeedback = ref("");

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
  };
});

const recentTransportActivities = computed(() => {
  return props.transportSnapshot?.activities.slice(0, 4) ?? [];
});

const latestTransportActivity = computed(() => {
  return recentTransportActivities.value[0] ?? null;
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
        title: "Restore Purchases",
        subtitle: "Reconnect private relay access and paid-circle onboarding state.",
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
      </template>

      <template v-else-if="settingId === 'restore'">
        <section class="hero-card">
          <div class="hero-copy">
            <h3>Restore Private Relays</h3>
            <p>Use the circle directory to reconnect invite-based and paid relay contexts. New circles will be stored in the local shell state immediately.</p>
          </div>
          <Tag value="Paid Circle Flow" severity="warn" rounded />
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
  color: #6a7d98;
  text-transform: uppercase;
  letter-spacing: 0.14em;
  font-size: 0.72rem;
}

.option-row,
.footer-actions,
.hero-tags,
.tag-row,
.list-tags {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.option-chip {
  padding: 10px 14px;
  border: 1px solid rgba(208, 218, 228, 0.95);
  border-radius: 999px;
  background: #fbfdff;
  color: #415772;
  cursor: pointer;
}

.option-chip.active {
  border-color: rgba(86, 136, 196, 0.82);
  background: linear-gradient(180deg, #f4f8ff 0%, #f5fbf8 100%);
  color: #16355c;
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
  background: #f7fafc;
}

.list-row {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  padding: 14px 16px;
  border-radius: 20px;
  background: #f7fafc;
}

.toggle-row strong,
.toggle-row p,
.info-row strong,
.info-row p,
.hero-copy h3,
.hero-copy p,
.list-copy strong,
.list-copy p,
.copy-feedback {
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
.list-copy p {
  color: #6d809a;
  line-height: 1.6;
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

.copy-feedback {
  color: #2f8c6a;
  font-size: 0.9rem;
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
