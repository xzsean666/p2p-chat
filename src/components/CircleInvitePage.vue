<script setup lang="ts">
import QRCode from "qrcode";
import { computed, onBeforeUnmount, ref, watch } from "vue";
import Button from "primevue/button";
import OverlayPageShell from "./OverlayPageShell.vue";

const props = defineProps<{
  circleName: string;
  inviteLink: string;
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "share"): void;
}>();

const copyState = ref<"idle" | "success" | "error">("idle");
const qrDataUrl = ref("");
let copyResetTimer: number | null = null;
let qrRequestSerial = 0;

const normalizedLink = computed(() => props.inviteLink.trim());
const inviteFallback = computed(() => {
  return normalizedLink.value || `circle:${props.circleName.trim().toLowerCase().replace(/\s+/g, "-")}`;
});
const circleInitials = computed(() => {
  const words = props.circleName.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) {
    return "CI";
  }

  return words
    .slice(0, 2)
    .map((word) => word[0]?.toUpperCase() ?? "")
    .join("");
});
const copyLabel = computed(() => {
  if (copyState.value === "success") {
    return "Copied";
  }

  if (copyState.value === "error") {
    return "Copy failed";
  }

  return "Copy link";
});
const copyHint = computed(() => {
  if (copyState.value === "success") {
    return "Invite link copied to clipboard.";
  }

  if (copyState.value === "error") {
    return "Clipboard access is unavailable on this device.";
  }

  return "Share directly or copy the invite link.";
});

onBeforeUnmount(() => {
  clearCopyStateTimer();
});

function clearCopyStateTimer() {
  if (copyResetTimer) {
    window.clearTimeout(copyResetTimer);
    copyResetTimer = null;
  }
}

function resetCopyStateSoon() {
  clearCopyStateTimer();
  copyResetTimer = window.setTimeout(() => {
    copyState.value = "idle";
    copyResetTimer = null;
  }, 1800);
}

async function copyInviteLink() {
  if (!inviteFallback.value) {
    copyState.value = "error";
    resetCopyStateSoon();
    return;
  }

  try {
    await navigator.clipboard.writeText(inviteFallback.value);
    copyState.value = "success";
  } catch {
    copyState.value = "error";
  }

  resetCopyStateSoon();
}

watch(
  inviteFallback,
  async (nextValue) => {
    const value = nextValue.trim();
    if (!value) {
      qrDataUrl.value = "";
      return;
    }

    const requestSerial = ++qrRequestSerial;

    try {
      const nextUrl = await QRCode.toDataURL(value, {
        errorCorrectionLevel: "M",
        margin: 1,
        width: 960,
        color: {
          dark: "#132742",
          light: "#FFFFFF",
        },
      });

      if (requestSerial === qrRequestSerial) {
        qrDataUrl.value = nextUrl;
      }
    } catch {
      if (requestSerial === qrRequestSerial) {
        qrDataUrl.value = "";
      }
    }
  },
  { immediate: true },
);
</script>

<template>
  <OverlayPageShell
    :title="circleName || 'Circle Invite'"
    subtitle="Scan, share or copy the invite details for this circle."
    @close="emit('close')"
  >
    <div class="invite-page">
      <section class="invite-hero">
        <div class="hero-badge">Current Circle</div>
        <h2>{{ circleName }}</h2>
        <p>Use the QR card below or share the invite link directly with new members.</p>
      </section>

      <section class="invite-card">
        <div class="qr-panel">
          <div class="qr-frame" aria-label="Invite QR display area">
            <img
              v-if="qrDataUrl"
              :src="qrDataUrl"
              class="qr-art"
              :alt="`Invite QR code for ${circleName}`"
            />
            <div v-else class="qr-art qr-art-fallback" aria-hidden="true"></div>

            <div class="qr-badge">
              <span>{{ circleInitials }}</span>
            </div>
          </div>

          <div class="qr-copy">
            <span class="eyebrow">Circle Invite</span>
            <strong>{{ circleName }}</strong>
            <p>{{ copyHint }}</p>
          </div>
        </div>

        <div class="link-panel">
          <div class="link-panel-header">
            <span class="eyebrow">Invite Link</span>
            <Button
              icon="pi pi-copy"
              :label="copyLabel"
              severity="secondary"
              text
              @click="copyInviteLink"
            />
          </div>

          <code>{{ inviteFallback }}</code>
        </div>
      </section>
    </div>

    <template #footer>
      <div class="invite-actions">
        <Button icon="pi pi-share-alt" label="Share invite" severity="contrast" @click="emit('share')" />
        <Button icon="pi pi-copy" :label="copyLabel" severity="secondary" @click="copyInviteLink" />
      </div>
    </template>
  </OverlayPageShell>
</template>

<style scoped>
.invite-page,
.invite-card,
.qr-panel,
.link-panel {
  display: grid;
}

.invite-page {
  gap: 18px;
}

.invite-hero {
  display: grid;
  gap: 10px;
  padding: 22px 22px 4px;
}

.hero-badge,
.eyebrow {
  color: #6f8099;
  font-size: 0.74rem;
  text-transform: uppercase;
  letter-spacing: 0.14em;
}

.invite-hero h2,
.invite-hero p,
.qr-copy p,
.link-panel code {
  margin: 0;
}

.invite-hero h2 {
  font-size: clamp(1.7rem, 5vw, 2.4rem);
  line-height: 1.02;
  color: #12233d;
}

.invite-hero p {
  max-width: 30rem;
  color: #6a7c95;
  line-height: 1.6;
}

.invite-card {
  gap: 16px;
  padding: 18px;
  border-radius: 28px;
  background:
    radial-gradient(circle at top left, rgba(130, 196, 255, 0.34), transparent 32%),
    radial-gradient(circle at bottom right, rgba(144, 238, 213, 0.3), transparent 28%),
    linear-gradient(180deg, #fcfdff 0%, #f1f7fd 100%);
  border: 1px solid rgba(142, 163, 191, 0.24);
  box-shadow: 0 20px 48px rgba(18, 35, 61, 0.12);
}

.qr-panel {
  gap: 16px;
  justify-items: center;
  padding: 14px 10px 8px;
}

.qr-frame {
  position: relative;
  display: grid;
  place-items: center;
  width: min(100%, 320px);
  aspect-ratio: 1;
  padding: 18px;
  border-radius: 32px;
  background:
    linear-gradient(145deg, rgba(17, 33, 58, 0.95), rgba(25, 54, 94, 0.94)),
    linear-gradient(180deg, #0f1e33 0%, #153056 100%);
  box-shadow:
    inset 0 1px 0 rgba(255, 255, 255, 0.08),
    0 18px 40px rgba(13, 31, 54, 0.24);
}

.qr-art {
  width: 100%;
  height: 100%;
  display: block;
  object-fit: contain;
  filter: drop-shadow(0 8px 14px rgba(19, 39, 66, 0.12));
}

.qr-art-fallback {
  border-radius: 18px;
  background:
    linear-gradient(135deg, rgba(19, 39, 66, 0.12), rgba(40, 77, 122, 0.08)),
    #ffffff;
}

.qr-badge {
  position: absolute;
  display: grid;
  place-items: center;
  width: 72px;
  height: 72px;
  border-radius: 24px;
  background: linear-gradient(135deg, #102744 0%, #1e5f83 100%);
  color: #f5fbff;
  border: 6px solid #ffffff;
  box-shadow: 0 10px 20px rgba(11, 28, 48, 0.22);
}

.qr-badge span {
  font-size: 1.05rem;
  font-weight: 700;
  letter-spacing: 0.08em;
}

.qr-copy {
  display: grid;
  gap: 6px;
  justify-items: center;
  text-align: center;
}

.qr-copy strong {
  font-size: 1.08rem;
  color: #16304e;
}

.qr-copy p {
  max-width: 19rem;
  color: #69809b;
  line-height: 1.55;
}

.link-panel {
  gap: 10px;
  padding: 16px;
  border-radius: 22px;
  background: rgba(255, 255, 255, 0.74);
  border: 1px solid rgba(145, 169, 199, 0.24);
}

.link-panel-header,
.invite-actions {
  display: flex;
  align-items: center;
  gap: 10px;
}

.link-panel-header {
  justify-content: space-between;
}

.link-panel code {
  display: block;
  padding: 14px 16px;
  border-radius: 18px;
  background: rgba(234, 242, 251, 0.9);
  color: #27435f;
  font-family: "IBM Plex Mono", monospace;
  font-size: 0.88rem;
  line-height: 1.65;
  word-break: break-all;
}

.invite-actions {
  flex-wrap: wrap;
  justify-content: stretch;
}

.invite-actions :deep(.p-button) {
  flex: 1 1 180px;
}

@media (min-width: 700px) {
  .invite-card {
    grid-template-columns: minmax(0, 1.1fr) minmax(0, 0.9fr);
    align-items: center;
  }

  .link-panel {
    align-self: stretch;
    align-content: start;
  }
}

@media (max-width: 520px) {
  .invite-hero {
    padding: 8px 4px 0;
  }

  .invite-card {
    padding: 14px;
    border-radius: 24px;
  }

  .qr-frame {
    width: 100%;
    border-radius: 26px;
  }

  .qr-badge {
    width: 62px;
    height: 62px;
    border-width: 5px;
  }

  .link-panel-header {
    align-items: flex-start;
    flex-direction: column;
  }
}
</style>
