<script setup lang="ts">
import jsQR from "jsqr";
import { computed, nextTick, onBeforeUnmount, ref } from "vue";
import InputText from "primevue/inputtext";
import OverlayPageShell from "./OverlayPageShell.vue";
import type { CircleItem, ContactItem } from "../types/chat";

const props = withDefaults(
  defineProps<{
    contacts: ContactItem[];
    currentCircleContactIds: string[];
    circle: CircleItem | null;
    mode: "chat" | "join-circle";
    submitting?: boolean;
    submitError?: string | null;
  }>(),
  {
    submitting: false,
    submitError: null,
  },
);

const emit = defineEmits<{
  (event: "close"): void;
  (event: "open-contact", contactId: string): void;
  (event: "select-contact", contactId: string): void;
  (event: "lookup-contact", query: string): void;
  (event: "join-circle", query: string): void;
}>();

const keyword = ref("");
const activePage = ref<"form" | "scanner">("form");
const scannerBusy = ref(false);
const scannerError = ref("");
const scannerIssue = ref<"permission-denied" | "camera-unavailable" | "generic-error" | null>(null);
const scannerVideo = ref<HTMLVideoElement | null>(null);
const scannerImageInput = ref<HTMLInputElement | null>(null);

let scannerStream: MediaStream | null = null;
let scannerFrameHandle = 0;
const scannerCanvas = typeof document !== "undefined" ? document.createElement("canvas") : null;
const scannerContext = scannerCanvas?.getContext("2d", { willReadFrequently: true }) ?? null;

const title = computed(() => {
  if (activePage.value === "scanner") {
    return "Scan QR Code";
  }

  return props.mode === "join-circle" ? "Join Circle" : "Add Friends";
});

const privacyNotice = computed(() => {
  return props.mode === "join-circle"
    ? "Enter a circle invite link or scan the invite QR code to join a circle."
    : "For privacy, users are hidden. You need an invite link or user ID to connect.";
});

const placeholder = computed(() => {
  return props.mode === "join-circle"
    ? "Enter circle invite link"
    : "Enter Invite Link or User ID (npub...)";
});

const canSubmit = computed(() => !props.submitting && keyword.value.trim().length > 0);
const showHeaderAction = computed(() => activePage.value === "form");
const submitButtonLabel = computed(() => {
  if (!props.submitting) {
    return "Next";
  }

  return props.mode === "join-circle" ? "Joining..." : "Finding...";
});
const submitStatusMessage = computed(() => {
  return props.mode === "join-circle" ? "Joining circle..." : "Looking up account...";
});
const submitErrorText = computed(() => props.submitError?.trim() ?? "");
const scannerRecovery = computed(() => {
  if (scannerIssue.value === "permission-denied") {
    return {
      title: "Camera access is turned off",
      description:
        "This app cannot start the scanner until camera access is allowed for this site or app.",
      note: "Open your browser or device privacy settings, allow camera access here, then return and retry.",
      steps: [
        "Open the site or app permissions for camera access.",
        "Switch camera access to Allow.",
        "Come back here and choose Retry Camera.",
      ],
    };
  }

  if (scannerIssue.value === "camera-unavailable") {
    return {
      title: "Camera is unavailable",
      description: "No usable camera is available in this browser or device context right now.",
      note: "If another app is using the camera, close it first. Otherwise switch to a context that exposes camera access, or continue from a QR image.",
      steps: [
        "Make sure no other app is using the camera.",
        "If this browser or device blocks camera access, switch to one that supports it.",
        "Use Open QR Image if you already have the code saved.",
      ],
    };
  }

  return null;
});

const scannerMessage = computed(() => {
  if (scannerBusy.value) {
    return "Opening camera...";
  }

  if (scannerError.value) {
    return scannerError.value;
  }

  return props.mode === "join-circle"
    ? "Align the invite QR code inside the frame to join the circle."
    : "Align the QR code inside the frame to add a friend.";
});

function submit() {
  if (props.submitting) {
    return;
  }

  const value = keyword.value.trim();
  if (!value) {
    return;
  }

  if (props.mode === "join-circle") {
    emit("join-circle", value);
    return;
  }

  emit("lookup-contact", value);
}

function resetScannerFrameLoop() {
  if (scannerFrameHandle) {
    window.cancelAnimationFrame(scannerFrameHandle);
    scannerFrameHandle = 0;
  }
}

function stopScannerStream() {
  resetScannerFrameLoop();

  if (scannerStream) {
    scannerStream.getTracks().forEach((track) => track.stop());
    scannerStream = null;
  }

  const video = scannerVideo.value;
  if (video) {
    video.pause();
    video.srcObject = null;
  }
}

function resetScannerStatus() {
  scannerError.value = "";
  scannerIssue.value = null;
}

function setScannerStatus(
  message: string,
  issue: "permission-denied" | "camera-unavailable" | "generic-error" | null = "generic-error",
) {
  scannerBusy.value = false;
  scannerError.value = message;
  scannerIssue.value = issue;
}

function closeScannerPage() {
  stopScannerStream();
  activePage.value = "form";
  scannerBusy.value = false;
  resetScannerStatus();
}

function applyScannedValue(value: string) {
  const resolved = value.trim();
  if (!resolved) {
    setScannerStatus("The QR code did not contain a usable value.");
    return;
  }

  keyword.value = resolved;
  closeScannerPage();
  submit();
}

function handleClose() {
  if (activePage.value === "scanner") {
    closeScannerPage();
    return;
  }

  emit("close");
}

function scheduleScannerFrame() {
  resetScannerFrameLoop();
  scannerFrameHandle = window.requestAnimationFrame(scanCameraFrame);
}

function scanCameraFrame() {
  const video = scannerVideo.value;
  if (activePage.value !== "scanner" || !video || !scannerCanvas || !scannerContext) {
    return;
  }

  if (video.readyState < HTMLMediaElement.HAVE_ENOUGH_DATA || !video.videoWidth || !video.videoHeight) {
    scheduleScannerFrame();
    return;
  }

  if (scannerCanvas.width !== video.videoWidth || scannerCanvas.height !== video.videoHeight) {
    scannerCanvas.width = video.videoWidth;
    scannerCanvas.height = video.videoHeight;
  }

  scannerContext.drawImage(video, 0, 0, scannerCanvas.width, scannerCanvas.height);
  const frame = scannerContext.getImageData(0, 0, scannerCanvas.width, scannerCanvas.height);
  const result = jsQR(frame.data, frame.width, frame.height, {
    inversionAttempts: "attemptBoth",
  });

  if (result?.data) {
    applyScannedValue(result.data);
    return;
  }

  scheduleScannerFrame();
}

async function startScannerCamera() {
  stopScannerStream();
  scannerBusy.value = true;
  resetScannerStatus();

  if (!navigator.mediaDevices?.getUserMedia) {
    setScannerStatus("Camera scanning is unavailable in this environment.", "camera-unavailable");
    return;
  }

  try {
    scannerStream = await navigator.mediaDevices.getUserMedia({
      video: {
        facingMode: "environment",
      },
      audio: false,
    });
  } catch (error) {
    if (error instanceof DOMException) {
      if (error.name === "NotAllowedError" || error.name === "SecurityError") {
        setScannerStatus("Camera access was denied.", "permission-denied");
        return;
      }

      if (
        error.name === "NotFoundError" ||
        error.name === "NotReadableError" ||
        error.name === "AbortError" ||
        error.name === "OverconstrainedError"
      ) {
        setScannerStatus("Unable to access a usable camera right now.", "camera-unavailable");
        return;
      }
    }

    setScannerStatus("Unable to open the camera. Open a QR image instead.");
    return;
  }

  await nextTick();

  const video = scannerVideo.value;
  if (!video) {
    setScannerStatus("Camera preview could not be created.");
    stopScannerStream();
    return;
  }

  video.srcObject = scannerStream;

  try {
    await video.play();
  } catch {
    setScannerStatus("Camera preview could not start.", "camera-unavailable");
    stopScannerStream();
    return;
  }

  scannerBusy.value = false;
  scheduleScannerFrame();
}

async function openScannerPage() {
  if (props.submitting) {
    return;
  }

  activePage.value = "scanner";
  await nextTick();
  await startScannerCamera();
}

function openScannerImagePicker() {
  if (props.submitting) {
    return;
  }

  scannerImageInput.value?.click();
}

function loadImage(url: string) {
  return new Promise<HTMLImageElement>((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("Image failed to load"));
    image.src = url;
  });
}

async function decodeQrImage(file: File) {
  if (!scannerCanvas || !scannerContext) {
    setScannerStatus("QR image decoding is unavailable in this environment.");
    return;
  }

  scannerBusy.value = true;
  resetScannerStatus();

  try {
    const imageUrl = URL.createObjectURL(file);
    try {
      const image = await loadImage(imageUrl);
      scannerCanvas.width = image.naturalWidth || image.width;
      scannerCanvas.height = image.naturalHeight || image.height;
      scannerContext.drawImage(image, 0, 0, scannerCanvas.width, scannerCanvas.height);
      const frame = scannerContext.getImageData(0, 0, scannerCanvas.width, scannerCanvas.height);
      const result = jsQR(frame.data, frame.width, frame.height, {
        inversionAttempts: "attemptBoth",
      });

      if (!result?.data) {
        setScannerStatus("No QR code was found in that image.");
        return;
      }

      applyScannedValue(result.data);
    } finally {
      URL.revokeObjectURL(imageUrl);
    }
  } catch {
    setScannerStatus("That image could not be scanned. Try another QR image.");
  } finally {
    scannerBusy.value = false;
  }
}

async function handleScannerImageSelected(event: Event) {
  const input = event.target as HTMLInputElement | null;
  const file = input?.files?.[0];
  if (!file) {
    return;
  }

  await decodeQrImage(file);
  input.value = "";
}

onBeforeUnmount(() => {
  stopScannerStream();
});
</script>

<template>
  <OverlayPageShell :title="title" @close="handleClose">
    <template v-if="showHeaderAction" #actions>
      <button
        type="button"
        class="header-next-button"
        :disabled="!canSubmit"
        @click="submit"
      >
        <span class="header-next-content">
          <i v-if="props.submitting" class="pi pi-spin pi-spinner"></i>
          <span>{{ submitButtonLabel }}</span>
        </span>
      </button>
    </template>

    <div class="find-page">
      <section v-if="activePage === 'form'" class="find-card">
        <div class="find-input-wrap">
          <InputText
            v-model="keyword"
            :placeholder="placeholder"
            autocomplete="off"
            :disabled="props.submitting"
            @keydown.enter.prevent="submit"
          />
        </div>

        <p class="find-notice">{{ privacyNotice }}</p>

        <p v-if="props.submitting" class="find-feedback find-feedback-state find-feedback-info">
          <i class="pi pi-spin pi-spinner"></i>
          <span>{{ submitStatusMessage }}</span>
        </p>
        <p v-else-if="submitErrorText" class="find-feedback find-feedback-state find-feedback-error" role="alert">
          <i class="pi pi-exclamation-circle"></i>
          <span>{{ submitErrorText }}</span>
        </p>

        <button
          type="button"
          class="scan-qr-button"
          :disabled="props.submitting"
          @click="openScannerPage"
        >
          <i class="pi pi-qrcode"></i>
          <span>Scan QR Code</span>
        </button>
      </section>

      <section v-else class="scanner-page">
        <div class="scanner-page-body">
          <div class="scanner-preview">
            <video ref="scannerVideo" class="scanner-video" autoplay muted playsinline></video>
            <div class="scanner-frame" aria-hidden="true"></div>
            <div v-if="scannerBusy || scannerError" class="scanner-overlay">
              <i class="pi pi-camera scanner-overlay-icon" />
            </div>
          </div>

          <div v-if="scannerRecovery" class="scanner-recovery-card" role="alert">
            <div class="scanner-recovery-header">
              <div class="scanner-recovery-badge">
                <i class="pi pi-cog scanner-recovery-badge-icon"></i>
              </div>
              <div class="scanner-recovery-copy">
                <h3>{{ scannerRecovery.title }}</h3>
                <p>{{ scannerRecovery.description }}</p>
              </div>
            </div>

            <p class="scanner-recovery-note">
              <i class="pi pi-sliders-h"></i>
              <span>{{ scannerRecovery.note }}</span>
            </p>

            <ol class="scanner-recovery-steps">
              <li v-for="step in scannerRecovery.steps" :key="step">
                {{ step }}
              </li>
            </ol>

            <div class="scanner-page-actions scanner-page-actions-recovery">
              <button type="button" class="scanner-link-button" @click="openScannerImagePicker">
                Open QR Image
              </button>
              <button
                type="button"
                class="scanner-link-button"
                :disabled="scannerBusy"
                @click="startScannerCamera"
              >
                Retry Camera
              </button>
            </div>
          </div>
          <p v-else class="find-feedback scanner-feedback">{{ scannerMessage }}</p>

          <div v-if="!scannerRecovery" class="scanner-page-actions">
            <button type="button" class="scanner-link-button" @click="openScannerImagePicker">
              Open QR Image
            </button>
            <button
              v-if="scannerError"
              type="button"
              class="scanner-link-button"
              :disabled="scannerBusy"
              @click="startScannerCamera"
            >
              Retry Camera
            </button>
          </div>

          <input
            ref="scannerImageInput"
            type="file"
            accept="image/*"
            class="scanner-file-input"
            @change="handleScannerImageSelected"
          />
        </div>
      </section>
    </div>
  </OverlayPageShell>
</template>

<style scoped>
.find-page {
  min-height: 100%;
  display: grid;
  align-content: start;
}

.find-card {
  width: min(560px, 100%);
  margin: 0 auto;
  display: grid;
  gap: 14px;
  padding-top: 8px;
}

.scanner-page {
  width: min(560px, 100%);
  margin: 0 auto;
  min-height: 100%;
}

.scanner-page-body {
  display: grid;
  gap: 16px;
  padding-top: 8px;
}

.header-next-button,
.scan-qr-button {
  appearance: none;
  border: 0;
  background: transparent;
  font: inherit;
}

.header-next-content,
.find-feedback-state {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

.header-next-button {
  padding: 6px 8px;
  color: #2d74dd;
  font-size: 0.96rem;
  font-weight: 600;
  cursor: pointer;
}

.header-next-button:disabled {
  color: #a9b4c2;
  cursor: default;
}

.find-input-wrap :deep(.p-inputtext) {
  width: 100%;
  padding: 14px 16px;
  border-radius: 15px;
  border: 1px solid #d9e1eb;
  background: #ffffff;
  box-shadow: none;
  color: #132032;
  font-size: 0.97rem;
}

.find-input-wrap :deep(.p-inputtext:enabled:focus) {
  border-color: #88afe8;
  box-shadow: 0 0 0 3px rgba(64, 118, 214, 0.12);
}

.find-notice,
.find-feedback {
  margin: 0;
  color: #6f7d90;
  line-height: 1.55;
}

.find-notice {
  font-size: 0.93rem;
}

.find-feedback {
  font-size: 0.88rem;
}

.find-feedback-state {
  padding: 12px 14px;
  border-radius: 14px;
  background: #f6f9fc;
}

.find-feedback-info {
  color: #54708d;
}

.find-feedback-error {
  color: #b3464b;
  background: #fff3f3;
}

.scanner-feedback {
  text-align: center;
}

.scanner-recovery-card {
  display: grid;
  gap: 14px;
  padding: 18px;
  border: 1px solid #d7e0eb;
  border-radius: 22px;
  background:
    linear-gradient(180deg, rgba(255, 255, 255, 0.98) 0%, rgba(241, 246, 252, 0.98) 100%);
  box-shadow: 0 12px 28px rgba(16, 29, 48, 0.08);
}

.scanner-recovery-header {
  display: flex;
  align-items: flex-start;
  gap: 12px;
}

.scanner-recovery-badge {
  width: 42px;
  height: 42px;
  flex: 0 0 auto;
  display: grid;
  place-items: center;
  border-radius: 14px;
  background: #ffffff;
  box-shadow: inset 0 0 0 1px #d7e1ee;
}

.scanner-recovery-badge-icon {
  color: #2d74dd;
  font-size: 1rem;
}

.scanner-recovery-copy {
  display: grid;
  gap: 6px;
}

.scanner-recovery-copy h3,
.scanner-recovery-copy p,
.scanner-recovery-note,
.scanner-recovery-steps {
  margin: 0;
}

.scanner-recovery-copy h3 {
  color: #172334;
  font-size: 1rem;
}

.scanner-recovery-copy p {
  color: #5d6f86;
  line-height: 1.55;
}

.scanner-recovery-note {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 12px 14px;
  border-radius: 16px;
  background: rgba(255, 255, 255, 0.92);
  color: #50637c;
  line-height: 1.5;
}

.scanner-recovery-steps {
  padding-left: 20px;
  color: #4c5d74;
  line-height: 1.5;
}

.scanner-recovery-steps li + li {
  margin-top: 8px;
}

.scan-qr-button {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 15px 18px;
  border: 1px solid #d8e0ea;
  border-radius: 16px;
  background: #f8fafc;
  color: #172334;
  font-weight: 600;
  cursor: pointer;
  transition:
    border-color 140ms ease,
    background-color 140ms ease,
    transform 140ms ease;
}

.scan-qr-button:hover {
  border-color: #c4cfdd;
  background: #f2f6fb;
}

.scan-qr-button:active {
  transform: translateY(1px);
}

.scan-qr-button i {
  font-size: 1.2rem;
  color: #2d74dd;
}

.scan-qr-button:disabled {
  border-color: #e2e8f0;
  background: #f8fafc;
  color: #9aa8bb;
  cursor: default;
  transform: none;
}

.scan-qr-button:disabled i {
  color: #9aa8bb;
}

.scanner-preview {
  position: relative;
  overflow: hidden;
  aspect-ratio: 1 / 1;
  border-radius: 22px;
  background: linear-gradient(180deg, #132033 0%, #223752 100%);
  box-shadow: inset 0 0 0 1px rgba(168, 185, 210, 0.26);
}

.scanner-video,
.scanner-overlay {
  position: absolute;
  inset: 0;
}

.scanner-video {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.scanner-overlay {
  display: grid;
  place-items: center;
  background: rgba(10, 19, 33, 0.45);
}

.scanner-overlay-icon {
  color: rgba(255, 255, 255, 0.92);
  font-size: 1.8rem;
}

.scanner-frame {
  position: absolute;
  inset: 14%;
  border: 2px solid rgba(255, 255, 255, 0.92);
  border-radius: 22px;
  box-shadow: 0 0 0 999px rgba(8, 15, 26, 0.18);
}

.scanner-page-actions {
  display: flex;
  justify-content: center;
  align-items: center;
  gap: 18px;
  flex-wrap: wrap;
}

.scanner-page-actions-recovery {
  justify-content: flex-start;
}

.scanner-link-button {
  appearance: none;
  padding: 0;
  border: 0;
  background: transparent;
  color: #2d74dd;
  font: inherit;
  font-weight: 600;
  cursor: pointer;
}

.scanner-link-button:disabled {
  color: #a9b4c2;
  cursor: default;
}

.scanner-file-input {
  display: none;
}

@media (max-width: 720px) {
  .find-card {
    width: 100%;
    padding-top: 0;
  }

  .scanner-page {
    width: 100%;
  }
}
</style>
