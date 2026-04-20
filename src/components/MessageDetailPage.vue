<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from "vue";
import Button from "primevue/button";
import Tag from "primevue/tag";
import OverlayPageShell from "./OverlayPageShell.vue";
import {
  fileMessageLocalPath,
  fileMessageMetaLabel,
  fileMessageRemoteUrl,
} from "../features/chat/fileMessageMeta";
import {
  imageMessageLocalPath,
  imageMessageMetaLabel,
  imageMessagePreviewUrl,
  imageMessageRemoteUrl,
} from "../features/chat/imageMessageMeta";
import {
  videoMessageLocalPath,
  videoMessageMetaLabel,
  videoMessagePreviewUrl,
  videoMessageRemoteUrl,
} from "../features/chat/videoMessageMeta";
import type { MessageItem, SessionItem } from "../types/chat";

const props = defineProps<{
  session: SessionItem | null;
  message: MessageItem | null;
  repliedMessage: MessageItem | null;
  canSendMessages: boolean;
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "retry-message", messageId: string): void;
  (event: "open-replied-message", messageId: string): void;
  (event: "open-attachment", messageId: string): void;
  (event: "reveal-attachment", messageId: string): void;
}>();

const copyFeedback = ref("");
let copyFeedbackTimer: ReturnType<typeof window.setTimeout> | null = null;

const messageTypeLabel = computed(() => {
  switch (props.message?.kind) {
    case "image":
      return "Image";
    case "video":
      return "Video";
    case "file":
      return "File";
    case "audio":
      return "Audio";
    case "system":
      return "System";
    default:
      return "Text";
  }
});

const authorLabel = computed(() => {
  if (!props.message) {
    return "Unknown";
  }

  switch (props.message.author) {
    case "me":
      return "You";
    case "peer":
      return props.session?.kind === "direct" ? props.session.name : "Peer";
    default:
      return "System";
  }
});

const deliveryLabel = computed(() => {
  switch (props.message?.deliveryStatus) {
    case "sending":
      return "Sending";
    case "failed":
      return "Failed";
    case "sent":
      return props.message.ackedAt ? "Delivered" : "Sent";
    default:
      return "Unavailable";
  }
});

const deliverySeverity = computed(() => {
  switch (props.message?.deliveryStatus) {
    case "sent":
      return "success";
    case "failed":
      return "danger";
    case "sending":
      return "warn";
    default:
      return "secondary";
  }
});

const syncSourceLabel = computed(() => {
  switch (props.message?.syncSource) {
    case "local":
      return "Local";
    case "relay":
      return "Relay";
    case "system":
      return "System";
    default:
      return "Unavailable";
  }
});

const signedEventCreatedAtLabel = computed(() => {
  if (!props.message?.signedNostrEvent) {
    return "";
  }

  return new Date(props.message.signedNostrEvent.createdAt * 1000).toLocaleString();
});

const signedEventJson = computed(() => {
  return props.message?.signedNostrEvent
    ? JSON.stringify(props.message.signedNostrEvent, null, 2)
    : "";
});

const replySummary = computed(() => {
  if (!props.message?.replyTo) {
    return "";
  }

  return `${props.message.replyTo.authorLabel}: ${props.message.replyTo.snippet}`;
});

function messageMetaSummary(message: MessageItem) {
  switch (message.kind) {
    case "file":
      return fileMessageMetaLabel(message) || message.meta || "";
    case "image":
      return imageMessageMetaLabel(message) || message.meta || "";
    case "video":
      return videoMessageMetaLabel(message) || message.meta || "";
    default:
      return message.meta ?? "";
  }
}

const contentSummary = computed(() => {
  if (!props.message) {
    return "";
  }

  switch (props.message.kind) {
    case "image":
    case "video":
      return messageMetaSummary(props.message)
        ? `${props.message.body} · ${messageMetaSummary(props.message)}`
        : props.message.body;
    case "file":
      return messageMetaSummary(props.message)
        ? `${props.message.body} · ${messageMetaSummary(props.message)}`
        : props.message.body;
    case "audio":
      return props.message.meta
        ? `Voice note · ${props.message.meta}`
        : "Voice note";
    default:
      return props.message.body || "Empty message";
  }
});

const attachmentLocalPath = computed(() => {
  if (!props.message) {
    return "";
  }

  switch (props.message.kind) {
    case "file":
      return fileMessageLocalPath(props.message);
    case "image":
      return imageMessageLocalPath(props.message);
    case "video":
      return videoMessageLocalPath(props.message);
    default:
      return "";
  }
});

const attachmentAvailable = computed(() => {
  if (!props.message) {
    return false;
  }

  switch (props.message.kind) {
    case "file":
      return !!attachmentLocalPath.value || !!fileMessageRemoteUrl(props.message);
    case "image":
      return !!attachmentLocalPath.value || !!imageMessageRemoteUrl(props.message);
    case "video":
      return !!attachmentLocalPath.value || !!videoMessageRemoteUrl(props.message);
    default:
      return false;
  }
});

async function copyValue(label: string, value: string | undefined) {
  if (!value) {
    return;
  }

  if (copyFeedbackTimer) {
    window.clearTimeout(copyFeedbackTimer);
    copyFeedbackTimer = null;
  }

  try {
    await navigator.clipboard.writeText(value);
    copyFeedback.value = `${label} copied`;
  } catch {
    copyFeedback.value = `Clipboard unavailable for ${label.toLowerCase()}`;
  }

  copyFeedbackTimer = window.setTimeout(() => {
    copyFeedback.value = "";
    copyFeedbackTimer = null;
  }, 2000);
}

onBeforeUnmount(() => {
  if (!copyFeedbackTimer) {
    return;
  }

  window.clearTimeout(copyFeedbackTimer);
});
</script>

<template>
  <OverlayPageShell
    title="Message Detail"
    subtitle="Inspect content, delivery state and signed event metadata."
    @close="emit('close')"
  >
    <div v-if="session && message" class="detail-body">
      <section class="hero-card">
        <div class="hero-copy">
          <span class="eyebrow">{{ messageTypeLabel }}</span>
          <h2>{{ contentSummary }}</h2>
          <p>{{ authorLabel }} · {{ message.time }}</p>
        </div>

        <div class="hero-tags">
          <Tag :value="deliveryLabel" :severity="deliverySeverity" rounded />
          <Tag :value="syncSourceLabel" severity="secondary" rounded />
        </div>
      </section>

      <section class="section-card">
        <div class="section-head">
          <div class="section-title">Content</div>
          <div class="section-actions">
            <Button
              v-if="attachmentAvailable"
              label="Open Attachment"
              text
              severity="contrast"
              @click="emit('open-attachment', message.id)"
            />
            <Button
              v-if="attachmentAvailable"
              label="Reveal in Folder"
              text
              severity="secondary"
              @click="emit('reveal-attachment', message.id)"
            />
            <Button
              v-if="message.body"
              label="Copy Body"
              text
              severity="contrast"
              @click="copyValue('Body', message.body)"
            />
          </div>
        </div>

        <div class="info-list">
          <div class="info-row block">
            <span class="label">Body</span>
            <p>{{ message.body || "Empty message" }}</p>
          </div>
          <div
            v-if="message.kind === 'image' && imageMessagePreviewUrl(message)"
            class="info-row block"
          >
            <span class="label">Preview</span>
            <img
              :src="imageMessagePreviewUrl(message)"
              :alt="message.body"
              class="media-preview"
            />
          </div>
          <div
            v-if="message.kind === 'video' && videoMessagePreviewUrl(message)"
            class="info-row block"
          >
            <span class="label">Preview</span>
            <video
              :src="videoMessagePreviewUrl(message)"
              class="media-preview"
              controls
              playsinline
              preload="metadata"
            />
          </div>
          <div v-if="message.meta" class="info-row block">
            <span class="label">Meta</span>
            <p>{{ messageMetaSummary(message) }}</p>
          </div>
          <div v-if="message.replyTo" class="info-row block">
            <span class="label">Reply</span>
            <p>{{ replySummary }}</p>
            <Button
              v-if="repliedMessage"
              label="Open Replied Message"
              text
              severity="secondary"
              class="inline-action"
              @click="emit('open-replied-message', repliedMessage.id)"
            />
            <p v-else class="muted-copy">The replied message is not in the loaded history yet.</p>
          </div>
        </div>
      </section>

      <section class="section-card">
        <div class="section-title">Delivery</div>
        <div class="info-list">
          <div class="info-row">
            <span class="label">Status</span>
            <strong>{{ deliveryLabel }}</strong>
          </div>
          <div class="info-row">
            <span class="label">Source</span>
            <strong>{{ syncSourceLabel }}</strong>
          </div>
          <div class="info-row">
            <span class="label">Remote ID</span>
            <code>{{ message.remoteId ?? "Unavailable" }}</code>
          </div>
          <div class="info-row">
            <span class="label">Acked At</span>
            <strong>{{ message.ackedAt ?? "Unavailable" }}</strong>
          </div>
        </div>
      </section>

      <section v-if="message.signedNostrEvent" class="section-card">
        <div class="section-head">
          <div class="section-title">Signed Event</div>
          <div class="section-actions">
            <Button
              label="Copy Event JSON"
              text
              severity="contrast"
              @click="copyValue('Event JSON', signedEventJson)"
            />
          </div>
        </div>

        <div class="info-list">
          <div class="info-row block">
            <span class="label">Event ID</span>
            <code>{{ message.signedNostrEvent.eventId }}</code>
          </div>
          <div class="info-row block">
            <span class="label">Pubkey</span>
            <code>{{ message.signedNostrEvent.pubkey }}</code>
          </div>
          <div class="info-row">
            <span class="label">Created At</span>
            <strong>{{ signedEventCreatedAtLabel }}</strong>
          </div>
          <div class="info-row">
            <span class="label">Kind</span>
            <strong>{{ message.signedNostrEvent.kind }}</strong>
          </div>
          <div class="info-row">
            <span class="label">Tags</span>
            <strong>{{ message.signedNostrEvent.tags.length }}</strong>
          </div>
          <div class="info-row block">
            <span class="label">Signature</span>
            <code>{{ message.signedNostrEvent.signature }}</code>
          </div>
        </div>
      </section>

      <p v-if="copyFeedback" class="copy-feedback">{{ copyFeedback }}</p>
    </div>

    <div v-else class="missing-state">
      <i class="pi pi-comment"></i>
      <p>This message is no longer available.</p>
    </div>

    <template v-if="message" #footer>
      <div class="detail-actions">
        <Button
          v-if="message.author === 'me' && message.deliveryStatus === 'failed'"
          icon="pi pi-refresh"
          label="Retry Message"
          severity="contrast"
          :disabled="!canSendMessages"
          @click="emit('retry-message', message.id)"
        />
        <Button
          v-if="message.remoteId"
          icon="pi pi-copy"
          label="Copy Remote ID"
          text
          severity="secondary"
          @click="copyValue('Remote ID', message.remoteId)"
        />
      </div>
    </template>
  </OverlayPageShell>
</template>

<style scoped>
.detail-body,
.hero-card,
.hero-copy,
.hero-tags,
.section-card,
.section-head,
.section-actions,
.info-list,
.detail-actions {
  display: grid;
}

.detail-body {
  gap: 18px;
}

.hero-card {
  gap: 14px;
  padding: 26px 24px;
  border-radius: 28px;
  background:
    radial-gradient(circle at top left, rgba(106, 168, 255, 0.2), transparent 24%),
    linear-gradient(180deg, #f7fbfe 0%, #eef5fb 100%);
}

.hero-copy {
  gap: 6px;
}

.hero-copy h2,
.hero-copy p,
.info-row p,
.copy-feedback,
.missing-state p,
code {
  margin: 0;
}

.eyebrow,
.section-title,
.label {
  color: #6a7d98;
  text-transform: uppercase;
  letter-spacing: 0.14em;
  font-size: 0.72rem;
}

.hero-copy h2 {
  font-size: 1.1rem;
  line-height: 1.45;
  color: #1f3756;
}

.hero-copy p,
.muted-copy {
  color: #6d809a;
  line-height: 1.65;
}

.hero-tags {
  grid-auto-flow: column;
  justify-content: start;
  gap: 8px;
}

.section-card {
  gap: 12px;
}

.media-preview {
  display: block;
  width: min(100%, 360px);
  max-height: 320px;
  object-fit: cover;
  border-radius: 18px;
  border: 1px solid rgba(157, 181, 211, 0.26);
  background: color-mix(in srgb, white 82%, #d8e8f8);
}

.section-head {
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  gap: 12px;
}

.section-actions,
.detail-actions {
  grid-auto-flow: column;
  gap: 8px;
  justify-content: start;
}

.info-list {
  gap: 10px;
}

.info-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 16px 18px;
  border-radius: 20px;
  background: #f7fafc;
}

.info-row.block {
  display: grid;
  justify-content: stretch;
}

.info-row strong,
.info-row p,
code {
  color: #415772;
  line-height: 1.65;
}

code {
  font-family: "IBM Plex Mono", monospace;
  word-break: break-all;
}

.inline-action {
  justify-self: start;
  margin-left: -0.5rem;
}

.copy-feedback {
  color: #4d6888;
  font-size: 0.92rem;
}

.detail-actions {
  align-items: center;
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

@media (max-width: 720px) {
  .section-head {
    grid-template-columns: 1fr;
  }

  .section-actions,
  .detail-actions {
    grid-auto-flow: row;
  }
}
</style>
