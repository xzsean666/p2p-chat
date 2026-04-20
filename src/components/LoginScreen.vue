<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import Button from "primevue/button";
import Checkbox from "primevue/checkbox";
import InputText from "primevue/inputtext";
import Message from "primevue/message";
import Tag from "primevue/tag";
import Textarea from "primevue/textarea";
import type {
  CircleItem,
  LoginAccessInput,
  LoginCircleSelectionMode,
  LoginCompletionInput,
  LoginMethod,
  RestorableCircleEntry,
  UserProfile,
} from "../types/chat";

const props = defineProps<{
  circles: CircleItem[];
  restorableCircles: RestorableCircleEntry[];
  profile: UserProfile;
}>();

const emit = defineEmits<{
  (event: "complete", payload: LoginCompletionInput): void;
}>();

const steps = [
  { label: "Entry", title: "Choose how to enter XChat" },
  { label: "Access", title: "Confirm your account access" },
  { label: "Profile", title: "Set the local profile shell" },
  { label: "Circle", title: "Choose the first circle context" },
] as const;

const slides = [
  {
    title: "Desktop onboarding now follows the original rhythm",
    text: "The rebuild keeps account entry, profile setup and circle selection as separate steps instead of collapsing everything into one button.",
  },
  {
    title: "Circle context stays visible before chat even opens",
    text: "Pick a saved relay, restore from an invite code or enter a custom endpoint before landing in the session shell.",
  },
  {
    title: "Signer and key import are first-class desktop paths",
    text: "The local flow now distinguishes quick start, existing Nostr credentials and remote signer style entry points.",
  },
];

const currentSlide = ref(0);
const currentStep = ref(0);
const selectedMethod = ref<LoginMethod>("quickStart");
const agreementAccepted = ref(true);
const accountKey = ref("");
const signerUri = ref("");
const displayName = ref(props.profile.name);
const handle = ref(props.profile.handle);
const profileStatus = ref(props.profile.status);
const circleMode = ref<LoginCircleSelectionMode>(props.circles.length ? "existing" : "custom");
const selectedCircleId = ref(props.circles[0]?.id ?? "");
const selectedRestoreRelay = ref(props.restorableCircles[0]?.relay ?? "");
const inviteCode = ref("");
const inviteName = ref("");
const customCircleName = ref("My Circle");
const customRelay = ref("");
let timer: number | undefined;

const selectedCircle = computed(() => {
  return props.circles.find((circle) => circle.id === selectedCircleId.value) ?? null;
});

const selectedRestorableCircle = computed(() => {
  return props.restorableCircles.find((circle) => circle.relay === selectedRestoreRelay.value) ?? null;
});

const hasRestorableCircles = computed(() => {
  return props.restorableCircles.length > 0;
});

const normalizedHandle = computed(() => {
  const raw = handle.value.trim().replace(/^@+/, "").toLowerCase().replace(/[^a-z0-9._-]+/g, "");
  return raw ? `@${raw}` : "";
});

const credentialValid = computed(() => {
  if (selectedMethod.value === "quickStart") {
    return true;
  }

  if (selectedMethod.value === "existingAccount") {
    const value = accountKey.value.trim();
    return /^(nsec|npub|bunker:\/\/)/i.test(value) || /^[a-f0-9]{32,}$/i.test(value);
  }

  return /^(bunker:\/\/|nostrconnect:\/\/)/i.test(signerUri.value.trim());
});

const canRestoreCirclesAfterLogin = computed(() => {
  return selectedMethod.value !== "quickStart" && hasRestorableCircles.value;
});

const profileValid = computed(() => {
  return displayName.value.trim().length >= 2 && normalizedHandle.value.length >= 4;
});

const circleValid = computed(() => {
  if (circleMode.value === "restore") {
    return canRestoreCirclesAfterLogin.value && !!selectedRestorableCircle.value;
  }

  if (circleMode.value === "existing") {
    return !!selectedCircle.value;
  }

  if (circleMode.value === "invite") {
    return inviteCode.value.trim().length >= 6;
  }

  return customCircleName.value.trim().length >= 2 && relayLooksValid(customRelay.value);
});

const stepValid = computed(() => {
  switch (currentStep.value) {
    case 0:
      return agreementAccepted.value;
    case 1:
      return credentialValid.value;
    case 2:
      return profileValid.value;
    default:
      return circleValid.value;
  }
});

const credentialTitle = computed(() => {
  switch (selectedMethod.value) {
    case "existingAccount":
      return "Import existing Nostr credentials";
    case "signer":
      return "Attach a remote signer";
    default:
      return "Quick start keeps the shell local";
  }
});

const credentialCopy = computed(() => {
  switch (selectedMethod.value) {
    case "existingAccount":
      return "Paste an `nsec`, `npub`, `bunker://` handoff or raw hex key. The desktop shell validates the shape before entry.";
    case "signer":
      return "Provide a `bunker://` or `nostrconnect://` signer URI. This rebuild stores the choice locally and keeps the next runtime integration boundary stable.";
    default:
      return "No remote credential is required for quick start. The desktop shell will continue with a local profile and circle selection flow.";
  }
});

const selectedCircleSummary = computed(() => {
  if (circleMode.value === "existing") {
    if (!selectedCircle.value) {
      return {
        title: "Choose a saved circle",
        detail: "Pick one of the saved relay contexts before opening the chat shell.",
      };
    }

    return {
      title: selectedCircle.value.name,
      detail: `${selectedCircle.value.relay} · ${selectedCircle.value.type}`,
    };
  }

  if (circleMode.value === "invite") {
    return {
      title: inviteName.value.trim() || "Invite Circle",
      detail: inviteCode.value.trim() || "Enter the invite code to create the relay context.",
    };
  }

  if (circleMode.value === "restore") {
    if (!selectedRestorableCircle.value) {
      return {
        title: "Restore Saved Circles",
        detail: hasRestorableCircles.value
          ? "Select an archived circle from the local restore catalog."
          : "No archived circles are available in the local restore catalog yet.",
      };
    }

    return {
      title: selectedRestorableCircle.value.name,
      detail: `${selectedRestorableCircle.value.relay} · ${selectedRestorableCircle.value.type}`,
    };
  }

  return {
    title: customCircleName.value.trim() || "Custom Relay",
    detail: customRelay.value.trim() || "Enter a relay endpoint such as wss://relay.example.com",
  };
});

watch(
  [selectedMethod, () => props.circles.length, () => props.restorableCircles.length],
  ([method, circleCount, restorableCount]) => {
    if (!circleCount && method !== "quickStart" && restorableCount > 0 && circleMode.value === "existing") {
      circleMode.value = "restore";
      return;
    }

    if (circleMode.value === "restore" && (method === "quickStart" || restorableCount === 0)) {
      circleMode.value = circleCount ? "existing" : "custom";
      return;
    }

    if (circleCount && circleMode.value === "restore" && restorableCount === 0) {
      circleMode.value = "existing";
    }
  },
  { immediate: true },
);

watch(
  () => props.circles,
  (circles) => {
    if (!circles.length) {
      selectedCircleId.value = "";
      if (circleMode.value === "existing") {
        circleMode.value =
          selectedMethod.value !== "quickStart" && props.restorableCircles.length
            ? "restore"
            : "custom";
      }
      return;
    }

    if (!circles.some((circle) => circle.id === selectedCircleId.value)) {
      selectedCircleId.value = circles[0]?.id ?? "";
    }
  },
  { deep: true, immediate: true },
);

watch(
  () => props.restorableCircles,
  (restorableCircles) => {
    if (!restorableCircles.length) {
      selectedRestoreRelay.value = "";
      if (circleMode.value === "restore") {
        circleMode.value = props.circles.length ? "existing" : "custom";
      }
      return;
    }

    if (!restorableCircles.some((circle) => circle.relay === selectedRestoreRelay.value)) {
      selectedRestoreRelay.value = restorableCircles[0]?.relay ?? "";
    }
  },
  { deep: true, immediate: true },
);

watch(
  () => props.profile,
  (profile) => {
    if (!displayName.value.trim()) {
      displayName.value = profile.name;
    }

    if (!handle.value.trim()) {
      handle.value = profile.handle;
    }

    if (!profileStatus.value.trim()) {
      profileStatus.value = profile.status;
    }
  },
  { deep: true, immediate: true },
);

onMounted(() => {
  timer = window.setInterval(() => {
    currentSlide.value = (currentSlide.value + 1) % slides.length;
  }, 3200);
});

onBeforeUnmount(() => {
  if (timer) {
    window.clearInterval(timer);
  }
});

function relayLooksValid(value: string) {
  const normalized = value.trim();
  return !!normalized && (normalized.includes("://") || normalized.includes("."));
}

function stepTone(index: number) {
  if (index < currentStep.value) {
    return "done";
  }

  if (index === currentStep.value) {
    return "active";
  }

  return "idle";
}

function selectMethod(method: LoginMethod) {
  selectedMethod.value = method;

  if (method === "quickStart") {
    if (!displayName.value.trim()) {
      displayName.value = props.profile.name;
    }

    if (!handle.value.trim()) {
      handle.value = props.profile.handle;
    }

    if (circleMode.value === "restore") {
      circleMode.value = props.circles.length ? "existing" : "custom";
    }

    return;
  }

  if (!props.circles.length && props.restorableCircles.length) {
    circleMode.value = "restore";
  }
}

function canJumpTo(index: number) {
  return index <= currentStep.value;
}

function goToStep(index: number) {
  if (canJumpTo(index)) {
    currentStep.value = index;
  }
}

function goBack() {
  currentStep.value = Math.max(currentStep.value - 1, 0);
}

function goNext() {
  if (!stepValid.value) {
    return;
  }

  currentStep.value = Math.min(currentStep.value + 1, steps.length - 1);
}

function buildInitials(name: string) {
  const tokens = name.trim().split(/\s+/).filter(Boolean);
  if (!tokens.length) {
    return "XC";
  }

  if (tokens.length === 1) {
    return tokens[0].slice(0, 2).toUpperCase();
  }

  return tokens
    .slice(0, 2)
    .map((token) => token.charAt(0))
    .join("")
    .toUpperCase();
}

function buildInviteCircleName() {
  const trimmed = inviteName.value.trim();
  if (trimmed) {
    return trimmed;
  }

  const suffix = inviteCode.value.trim().slice(0, 6).toUpperCase();
  return suffix ? `Invite ${suffix}` : "Invite Circle";
}

function restoreTypeTone(type: RestorableCircleEntry["type"]) {
  if (type === "paid") {
    return "warn";
  }

  if (type === "custom") {
    return "contrast";
  }

  return "secondary";
}

function archivedAtCopy(value: string) {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }

  return parsed.toLocaleString();
}

function deriveExistingAccountAccessKind(value: string): LoginAccessInput["kind"] {
  const trimmed = value.trim().toLowerCase();
  if (trimmed.startsWith("nsec")) {
    return "nsec";
  }

  if (trimmed.startsWith("npub")) {
    return "npub";
  }

  if (trimmed.startsWith("bunker://")) {
    return "bunker";
  }

  return "hexKey";
}

function deriveSignerAccessKind(value: string): LoginAccessInput["kind"] {
  const trimmed = value.trim().toLowerCase();
  if (trimmed.startsWith("nostrconnect://")) {
    return "nostrConnect";
  }

  return "bunker";
}

function submit() {
  if (!stepValid.value) {
    return;
  }

  const access: LoginAccessInput =
    selectedMethod.value === "quickStart"
      ? {
          kind: "localProfile",
        }
      : selectedMethod.value === "existingAccount"
        ? {
            kind: deriveExistingAccountAccessKind(accountKey.value),
            value: accountKey.value.trim(),
          }
        : {
            kind: deriveSignerAccessKind(signerUri.value),
            value: signerUri.value.trim(),
          };

  const payload: LoginCompletionInput = {
    method: selectedMethod.value,
    access,
    userProfile: {
      name: displayName.value.trim(),
      handle: normalizedHandle.value,
      initials: buildInitials(displayName.value),
      status: profileStatus.value.trim() || "Circle member",
    },
    circleSelection:
      circleMode.value === "existing"
        ? {
            mode: "existing",
            circleId: selectedCircleId.value,
          }
        : circleMode.value === "invite"
          ? {
            mode: "invite",
            inviteCode: inviteCode.value.trim(),
            name: buildInviteCircleName(),
          }
        : circleMode.value === "restore"
          ? {
              mode: "restore",
              relay: selectedRestorableCircle.value?.relay,
            }
          : {
              mode: "custom",
              name: customCircleName.value.trim(),
              relay: customRelay.value.trim(),
            },
  };

  emit("complete", payload);
}
</script>

<template>
  <section class="login-screen">
    <div class="login-card">
      <div class="login-hero">
        <p class="eyebrow">Welcome to XChat</p>
        <h1>{{ slides[currentSlide].title }}</h1>
        <p class="hero-copy">{{ slides[currentSlide].text }}</p>

          <div class="hero-metrics">
            <div class="metric-chip">
              <strong>{{ props.circles.length }}</strong>
              <span>saved circles</span>
            </div>
            <div class="metric-chip">
              <strong>{{ props.restorableCircles.length }}</strong>
              <span>restore entries</span>
            </div>
            <div class="metric-chip">
              <strong>4</strong>
              <span>entry steps</span>
            </div>
          </div>

        <div class="slide-markers">
          <button
            v-for="(_, index) in slides"
            :key="index"
            type="button"
            :class="['marker', { active: index === currentSlide }]"
            @click="currentSlide = index"
          />
        </div>
      </div>

      <div class="login-flow">
        <div class="stepper-row">
          <button
            v-for="(step, index) in steps"
            :key="step.label"
            type="button"
            :disabled="!canJumpTo(index)"
            :class="['step-pill', stepTone(index)]"
            @click="goToStep(index)"
          >
            <span>{{ step.label }}</span>
            <strong>{{ index + 1 }}</strong>
          </button>
        </div>

        <div class="flow-copy">
          <p class="flow-kicker">Onboarding</p>
          <h2>{{ steps[currentStep].title }}</h2>
        </div>

        <div class="flow-body">
          <template v-if="currentStep === 0">
            <div class="selection-grid">
              <button
                type="button"
                :class="['selection-card', { active: selectedMethod === 'quickStart' }]"
                @click="selectMethod('quickStart')"
              >
                <div class="card-head">
                  <strong>Quick Start</strong>
                  <Tag value="Local" severity="success" rounded />
                </div>
                <p>Generate a desktop-local identity shell, then continue into profile setup and circle selection.</p>
              </button>

              <button
                type="button"
                :class="['selection-card', { active: selectedMethod === 'existingAccount' }]"
                @click="selectMethod('existingAccount')"
              >
                <div class="card-head">
                  <strong>Existing Nostr Account</strong>
                  <Tag value="Import" severity="info" rounded />
                </div>
                <p>Bring in a key or imported account handoff before opening the main chat shell.</p>
              </button>

              <button
                type="button"
                :class="['selection-card', { active: selectedMethod === 'signer' }]"
                @click="selectMethod('signer')"
              >
                <div class="card-head">
                  <strong>Remote Signer</strong>
                  <Tag value="NIP-46" severity="warn" rounded />
                </div>
                <p>Keep signing outside the desktop shell and attach through a signer URI style flow.</p>
              </button>
            </div>

            <div class="agreement-row">
              <Checkbox v-model="agreementAccepted" binary input-id="agreement" />
              <label for="agreement">
                Continue with the local privacy policy and terms flow enabled.
              </label>
            </div>
          </template>

          <template v-else-if="currentStep === 1">
            <section class="section-card">
              <div class="section-copy">
                <strong>{{ credentialTitle }}</strong>
                <p>{{ credentialCopy }}</p>
              </div>

              <Message v-if="selectedMethod === 'quickStart'" severity="info" :closable="false">
                Quick start does not require a key today. The profile and circle choices will be stored locally so the next runtime/auth integration can reuse the same shell contract.
              </Message>

              <template v-else-if="selectedMethod === 'existingAccount'">
                <label class="field-label" for="account-key">Account key or import handoff</label>
                <Textarea
                  id="account-key"
                  v-model="accountKey"
                  rows="5"
                  auto-resize
                  class="wide-input"
                  placeholder="nsec1..., npub1..., bunker://..., or raw hex"
                />
                <Message v-if="accountKey.trim() && !credentialValid" severity="warn" :closable="false">
                  The input does not look like a supported key or import handoff yet.
                </Message>
              </template>

              <template v-else>
                <label class="field-label" for="signer-uri">Signer URI</label>
                <InputText
                  id="signer-uri"
                  v-model="signerUri"
                  class="wide-input"
                  placeholder="bunker://... or nostrconnect://..."
                />
                <Message v-if="signerUri.trim() && !credentialValid" severity="warn" :closable="false">
                  Use a `bunker://` or `nostrconnect://` signer URI.
                </Message>
              </template>
            </section>
          </template>

          <template v-else-if="currentStep === 2">
            <section class="section-card profile-card">
              <div class="profile-preview">
                <div class="preview-avatar">{{ buildInitials(displayName || "XC") }}</div>
                <div class="preview-copy">
                  <strong>{{ displayName || "Your Name" }}</strong>
                  <p>{{ normalizedHandle || "@handle" }}</p>
                  <span>{{ profileStatus || "Circle member" }}</span>
                </div>
              </div>

              <label class="field-label" for="display-name">Display name</label>
              <InputText
                id="display-name"
                v-model="displayName"
                class="wide-input"
                placeholder="Sean Chen"
              />

              <label class="field-label" for="handle">Handle</label>
              <InputText
                id="handle"
                v-model="handle"
                class="wide-input"
                placeholder="@seanchen"
              />
              <p class="field-help">Saved as {{ normalizedHandle || "@handle" }}</p>

              <label class="field-label" for="status-line">Status line</label>
              <InputText
                id="status-line"
                v-model="profileStatus"
                class="wide-input"
                placeholder="Circle owner"
              />
            </section>
          </template>

          <template v-else>
            <section class="section-card">
              <div class="mode-row">
                <button
                  v-if="selectedMethod !== 'quickStart'"
                  type="button"
                  :class="['mode-chip', { active: circleMode === 'restore' }]"
                  :disabled="!canRestoreCirclesAfterLogin"
                  @click="circleMode = 'restore'"
                >
                  Restore Catalog
                </button>
                <button
                  type="button"
                  :class="['mode-chip', { active: circleMode === 'existing' }]"
                  :disabled="!props.circles.length"
                  @click="circleMode = 'existing'"
                >
                  Saved Circles
                </button>
                <button
                  type="button"
                  :class="['mode-chip', { active: circleMode === 'invite' }]"
                  @click="circleMode = 'invite'"
                >
                  Invite Code
                </button>
                <button
                  type="button"
                  :class="['mode-chip', { active: circleMode === 'custom' }]"
                  @click="circleMode = 'custom'"
                >
                  Custom Relay
                </button>
              </div>

              <div v-if="circleMode === 'existing'" class="selection-grid">
                <button
                  v-for="circle in props.circles"
                  :key="circle.id"
                  type="button"
                  :class="['selection-card', { active: selectedCircleId === circle.id }]"
                  @click="selectedCircleId = circle.id"
                >
                  <div class="card-head">
                    <strong>{{ circle.name }}</strong>
                    <Tag :value="circle.type" :severity="circle.type === 'paid' ? 'warn' : 'secondary'" rounded />
                  </div>
                  <p>{{ circle.description }}</p>
                  <span class="card-meta">{{ circle.relay }} · {{ circle.status }}</span>
                </button>

                <Message v-if="!props.circles.length" severity="secondary" :closable="false">
                  No saved circles are available yet. Switch to invite code or custom relay.
                </Message>
              </div>

              <div v-else-if="circleMode === 'restore'" class="field-grid">
                <Message
                  v-if="selectedMethod === 'quickStart'"
                  severity="secondary"
                  :closable="false"
                >
                  Restore catalog is only available for existing-account and signer entry paths.
                </Message>
                <template v-else-if="props.restorableCircles.length">
                  <Message severity="info" :closable="false">
                    Pick one archived circle from the local restore catalog. The desktop shell will restore it immediately after authentication completes.
                  </Message>
                  <div class="selection-grid">
                    <button
                      v-for="circle in props.restorableCircles"
                      :key="circle.relay"
                      type="button"
                      :class="['selection-card', { active: selectedRestoreRelay === circle.relay }]"
                      @click="selectedRestoreRelay = circle.relay"
                    >
                      <div class="card-head">
                        <strong>{{ circle.name }}</strong>
                        <Tag :value="circle.type" :severity="restoreTypeTone(circle.type)" rounded />
                      </div>
                      <p>{{ circle.description || 'No archived description available.' }}</p>
                      <span class="card-meta">{{ circle.relay }}</span>
                      <span class="card-meta">Archived {{ archivedAtCopy(circle.archivedAt) }}</span>
                    </button>
                  </div>
                </template>
                <Message v-else severity="secondary" :closable="false">
                  No archived circles are available yet. Use invite code or custom relay instead.
                </Message>
              </div>

              <div v-else-if="circleMode === 'invite'" class="field-grid">
                <label class="field-label" for="invite-code">Invite code</label>
                <InputText
                  id="invite-code"
                  v-model="inviteCode"
                  class="wide-input"
                  placeholder="circle://..., invite://..., or invitation code"
                />

                <label class="field-label" for="invite-name">Circle name</label>
                <InputText
                  id="invite-name"
                  v-model="inviteName"
                  class="wide-input"
                  placeholder="Invite Circle"
                />
              </div>

              <div v-else class="field-grid">
                <label class="field-label" for="custom-circle-name">Circle name</label>
                <InputText
                  id="custom-circle-name"
                  v-model="customCircleName"
                  class="wide-input"
                  placeholder="My Circle"
                />

                <label class="field-label" for="custom-relay">Relay endpoint</label>
                <InputText
                  id="custom-relay"
                  v-model="customRelay"
                  class="wide-input"
                  placeholder="wss://relay.example.com"
                />
              </div>

              <div class="summary-card">
                <strong>{{ selectedCircleSummary.title }}</strong>
                <p>{{ selectedCircleSummary.detail }}</p>
              </div>
            </section>
          </template>
        </div>

        <div class="footer-actions">
          <Button
            label="Back"
            text
            severity="secondary"
            :disabled="currentStep === 0"
            @click="goBack"
          />
          <Button
            v-if="currentStep < steps.length - 1"
            label="Continue"
            severity="contrast"
            :disabled="!stepValid"
            @click="goNext"
          />
          <Button
            v-else
            label="Enter XChat"
            severity="contrast"
            :disabled="!stepValid"
            @click="submit"
          />
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.login-screen {
  display: grid;
  place-items: center;
  min-height: calc(100vh - 36px);
}

.login-card {
  display: grid;
  grid-template-columns: minmax(0, 1.1fr) minmax(360px, 0.9fr);
  gap: 20px;
  width: min(1180px, calc(100vw - 36px));
  min-height: min(760px, calc(100vh - 36px));
  padding: 22px;
  border-radius: 32px;
  background: rgba(255, 255, 255, 0.92);
  border: 1px solid rgba(210, 220, 232, 0.9);
  box-shadow: 0 24px 60px rgba(24, 46, 84, 0.1);
}

.login-hero,
.login-flow,
.hero-metrics,
.flow-body,
.section-card,
.selection-grid,
.field-grid,
.profile-card {
  display: grid;
}

.login-hero,
.login-flow {
  border-radius: 28px;
}

.login-hero {
  align-content: end;
  gap: 18px;
  padding: 34px;
  background:
    radial-gradient(circle at top left, rgba(106, 168, 255, 0.28), transparent 24%),
    radial-gradient(circle at right bottom, rgba(76, 215, 166, 0.24), transparent 20%),
    linear-gradient(180deg, #233966 0%, #1a2947 100%);
  color: #f5f8fe;
}

.login-flow {
  gap: 18px;
  padding: 28px;
  background: linear-gradient(180deg, #f8fbfe 0%, #f2f7fb 100%);
}

.eyebrow,
.hero-copy,
.flow-kicker,
.flow-copy h2,
.section-copy p,
.section-copy strong,
.agreement-row label,
.field-help,
.summary-card p {
  margin: 0;
}

.eyebrow,
.flow-kicker {
  text-transform: uppercase;
  letter-spacing: 0.18em;
  font-size: 0.76rem;
}

.eyebrow {
  color: rgba(245, 248, 254, 0.74);
}

.flow-kicker {
  color: #6b7d97;
}

.login-hero h1,
.flow-copy h2 {
  margin: 0;
}

.login-hero h1 {
  font-size: clamp(2.2rem, 4.8vw, 4rem);
  line-height: 0.98;
  letter-spacing: -0.05em;
  max-width: 12ch;
}

.hero-copy,
.section-copy p,
.agreement-row label,
.field-help,
.summary-card p {
  line-height: 1.65;
}

.hero-copy {
  max-width: 44ch;
  color: rgba(245, 248, 254, 0.84);
}

.hero-metrics {
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px;
}

.metric-chip {
  display: grid;
  gap: 4px;
  padding: 14px 16px;
  border-radius: 18px;
  background: rgba(255, 255, 255, 0.09);
}

.metric-chip strong {
  font-size: 1.1rem;
}

.metric-chip span {
  color: rgba(245, 248, 254, 0.72);
  font-size: 0.85rem;
}

.slide-markers,
.stepper-row,
.card-head,
.agreement-row,
.footer-actions,
.mode-row {
  display: flex;
  align-items: center;
}

.slide-markers {
  gap: 8px;
}

.marker {
  width: 34px;
  height: 6px;
  border: 0;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.22);
  cursor: pointer;
}

.marker.active {
  background: #ffffff;
}

.stepper-row,
.mode-row,
.footer-actions {
  gap: 10px;
  flex-wrap: wrap;
}

.step-pill,
.mode-chip,
.selection-card {
  border: 1px solid rgba(208, 218, 228, 0.95);
  background: #ffffff;
}

.step-pill {
  display: flex;
  justify-content: space-between;
  gap: 10px;
  min-width: 110px;
  padding: 10px 14px;
  border-radius: 999px;
  color: #52667f;
  cursor: pointer;
}

.step-pill strong {
  font-size: 0.86rem;
}

.step-pill.active {
  border-color: rgba(83, 132, 193, 0.8);
  background: linear-gradient(180deg, #f3f8ff 0%, #f5fbf8 100%);
  color: #17345c;
}

.step-pill.done {
  border-color: rgba(75, 164, 126, 0.48);
  color: #2b6b55;
}

.step-pill:disabled {
  cursor: default;
  opacity: 0.72;
}

.flow-copy {
  display: grid;
  gap: 8px;
}

.flow-copy h2 {
  font-size: 1.7rem;
  color: #18253d;
}

.flow-body {
  gap: 16px;
  min-height: 0;
}

.section-card,
.summary-card {
  gap: 12px;
  padding: 18px;
  border-radius: 24px;
  background: #ffffff;
  border: 1px solid rgba(214, 223, 233, 0.84);
}

.selection-grid,
.field-grid,
.profile-card {
  gap: 12px;
}

.selection-card {
  display: grid;
  gap: 10px;
  width: 100%;
  padding: 18px;
  border-radius: 22px;
  color: #17345c;
  text-align: left;
  cursor: pointer;
}

.selection-card.active {
  border-color: rgba(83, 132, 193, 0.82);
  background: linear-gradient(180deg, #f4f8ff 0%, #f5fbf8 100%);
}

.selection-card p,
.card-meta,
.preview-copy p,
.preview-copy span {
  margin: 0;
  color: #6c8098;
}

.card-head {
  justify-content: space-between;
  gap: 12px;
}

.agreement-row {
  gap: 10px;
  color: #6c8098;
  font-size: 0.92rem;
}

.field-label {
  color: #4d6178;
  font-size: 0.9rem;
  font-weight: 600;
}

.wide-input {
  width: 100%;
}

.field-help {
  color: #6c8098;
  font-size: 0.88rem;
}

.profile-preview {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 14px;
  border-radius: 20px;
  background: linear-gradient(180deg, #f4f8ff 0%, #f5fbf8 100%);
}

.preview-avatar {
  display: grid;
  place-items: center;
  width: 50px;
  height: 50px;
  border-radius: 999px;
  background: linear-gradient(135deg, #dce9ff 0%, #d9f9ef 100%);
  color: #16355c;
  font-weight: 700;
}

.preview-copy {
  display: grid;
  gap: 2px;
}

.mode-chip {
  padding: 10px 14px;
  border-radius: 999px;
  color: #52667f;
  cursor: pointer;
}

.mode-chip.active {
  border-color: rgba(83, 132, 193, 0.82);
  background: linear-gradient(180deg, #f4f8ff 0%, #f5fbf8 100%);
  color: #17345c;
}

.mode-chip:disabled {
  cursor: not-allowed;
  opacity: 0.52;
}

.summary-card strong {
  color: #17345c;
}

.footer-actions {
  justify-content: space-between;
  margin-top: auto;
}

@media (max-width: 980px) {
  .login-card {
    grid-template-columns: 1fr;
    min-height: auto;
  }

  .hero-metrics {
    grid-template-columns: 1fr;
  }

  .login-screen {
    min-height: calc(100vh - 24px);
  }
}
</style>
