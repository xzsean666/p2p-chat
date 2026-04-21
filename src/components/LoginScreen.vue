<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import InputText from "primevue/inputtext";
import Textarea from "primevue/textarea";
import onboardingWelcomeImage from "../../tmp/xchat-app-main/packages/business_modules/ox_login/assets/images/material_onboarding-welcome.png";
import onboardingNostrImage from "../../tmp/xchat-app-main/packages/business_modules/ox_login/assets/images/material_onboarding-nostr.png";
import onboardingCircleImage from "../../tmp/xchat-app-main/packages/business_modules/ox_login/assets/images/material_onboarding-circle.png";
import onboardingRelaysImage from "../../tmp/xchat-app-main/packages/business_modules/ox_login/assets/images/material_onboarding-relays.png";
import type {
  CircleItem,
  LoginAccessInput,
  LoginCircleSelectionMode,
  LoginCompletionInput,
  LoginMethod,
  RestorableCircleEntry,
  UserProfile,
} from "../types/chat";

type CircleSheetMode = "invite" | "custom";
type InfoSheetKind = "nostr" | "relay";

const props = defineProps<{
  circles: CircleItem[];
  restorableCircles: RestorableCircleEntry[];
  profile: UserProfile;
}>();

const emit = defineEmits<{
  (event: "complete", payload: LoginCompletionInput): void;
}>();

const slides = [
  {
    title: "Welcome to XChat",
    text: "Welcome to our secure, decentralized communication platform. Your journey to true privacy starts here.",
    image: onboardingWelcomeImage,
  },
  {
    title: "Open Protocol",
    text: "Built on the Nostr protocol. Decentralized communication where you control your identity.",
    image: onboardingNostrImage,
  },
  {
    title: "Private Circles",
    text: "Switch seamlessly between different circles. Your conversations stay private.",
    image: onboardingCircleImage,
  },
  {
    title: "Custom Servers",
    text: "Customize your relays and file servers. You have complete control over where your data is stored.",
    image: onboardingRelaysImage,
  },
] as const;

const NOSTR_BECH32_DATA_CHARS = "[023456789acdefghjklmnpqrstuvwxyz]+";
const NSEC_PATTERN = new RegExp(`^nsec1${NOSTR_BECH32_DATA_CHARS}$`, "i");
const NPUB_PATTERN = new RegExp(`^npub1${NOSTR_BECH32_DATA_CHARS}$`, "i");
const HEX_KEY_PATTERN = /^[a-f0-9]{64}$/i;
const HEX_PUBKEY_PATTERN = /^[a-f0-9]{64}$/i;

const currentSlide = ref(0);
const currentStep = ref(0);
const selectedMethod = ref<LoginMethod>("quickStart");
const accountKey = ref("");
const handle = ref(props.profile.handle);
const profileStatus = ref(props.profile.status);
const circleMode = ref<LoginCircleSelectionMode>(defaultCircleModeForMethod("quickStart"));
const selectedCircleId = ref(props.circles[0]?.id ?? "");
const selectedRestoreRelay = ref(props.restorableCircles[0]?.relay ?? "");
const inviteCode = ref("");
const inviteName = ref("");
const customCircleName = ref("");
const customRelay = ref("");
const activeCircleSheet = ref<CircleSheetMode | null>(null);
const activeInfoSheet = ref<InfoSheetKind | null>(null);

const seededName = splitName(props.profile.name);
const firstName = ref(seededName.first);
const lastName = ref(seededName.last);

let timer: number | undefined;

const selectedRestorableCircle = computed(() => {
  return props.restorableCircles.find((circle) => circle.relay === selectedRestoreRelay.value) ?? null;
});

const hasRestorableCircles = computed(() => {
  return props.restorableCircles.length > 0;
});

const displayName = computed(() => {
  return [firstName.value.trim(), lastName.value.trim()].filter(Boolean).join(" ").trim();
});

const normalizedHandle = computed(() => {
  const generated = `${firstName.value}${lastName.value}`.trim();
  const preferred = handle.value.trim() || generated || "xchatuser";
  const raw = preferred.replace(/^@+/, "").toLowerCase().replace(/[^a-z0-9._-]+/g, "");
  return raw ? `@${raw}` : "@xchatuser";
});

const normalizedAccountKey = computed(() => {
  return accountKey.value.trim();
});

const accountKeyAccessKind = computed<LoginAccessInput["kind"] | null>(() => {
  if (selectedMethod.value !== "existingAccount") {
    return null;
  }

  return deriveAccountKeyAccessKind(normalizedAccountKey.value);
});

const credentialValid = computed(() => {
  if (selectedMethod.value === "quickStart") {
    return true;
  }

  return accountKeyAccessKind.value !== null;
});

const remoteSignerSelected = computed(() => {
  return accountKeyAccessKind.value === "bunker" || accountKeyAccessKind.value === "nostrConnect";
});

const nostrConnectSelected = computed(() => {
  return accountKeyAccessKind.value === "nostrConnect";
});

const invalidRemoteSignerHintVisible = computed(() => {
  const value = normalizedAccountKey.value.toLowerCase();
  if (accountKeyAccessKind.value !== null) {
    return false;
  }

  return value.startsWith("bunker://") || value.startsWith("nostrconnect://");
});

const npubSelected = computed(() => {
  return accountKeyAccessKind.value === "npub";
});

const invalidNsecHintVisible = computed(() => {
  const value = normalizedAccountKey.value.toLowerCase();
  return value.startsWith("nsec") && value.length >= 10 && accountKeyAccessKind.value !== "nsec";
});

const canRestoreCirclesAfterLogin = computed(() => {
  return selectedMethod.value !== "quickStart" && hasRestorableCircles.value;
});

const canReuseExistingCircleImmediately = computed(() => {
  return selectedMethod.value === "existingAccount" && props.circles.length > 0;
});

const needsCircleSelectionStep = computed(() => {
  if (selectedMethod.value === "quickStart") {
    return true;
  }

  return !canReuseExistingCircleImmediately.value;
});

const profileValid = computed(() => {
  return displayName.value.length >= 2;
});

const circleStepReady = computed(() => {
  if (circleMode.value === "restore") {
    return canRestoreCirclesAfterLogin.value && !!selectedRestorableCircle.value;
  }

  if (circleMode.value === "existing") {
    return !!selectedCircleId.value;
  }

  return true;
});

const circleSheetValid = computed(() => {
  if (activeCircleSheet.value === "invite") {
    return inviteCode.value.trim().length >= 6;
  }

  if (activeCircleSheet.value === "custom") {
    return relayLooksValid(customRelay.value);
  }

  return false;
});

const normalizedCustomRelayPreview = computed(() => {
  return normalizeRelayLikeValue(customRelay.value);
});

const stepValid = computed(() => {
  switch (currentStep.value) {
    case 1:
      return credentialValid.value;
    case 2:
      return profileValid.value;
    case 3:
      return circleStepReady.value;
    default:
      return true;
  }
});

const primaryActionLabel = computed(() => {
  if (currentStep.value === 1) {
    return "LOGIN";
  }

  if (currentStep.value === 3) {
    return "Connect";
  }

  return "Continue";
});

watch(
  [selectedMethod, () => props.restorableCircles.length],
  ([method, restorableCount]) => {
    if (circleMode.value === "restore" && (method === "quickStart" || restorableCount === 0)) {
      circleMode.value = "invite";
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
          selectedMethod.value !== "quickStart" && props.restorableCircles.length ? "restore" : "invite";
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
        circleMode.value = "invite";
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
    if (!firstName.value.trim() && !lastName.value.trim()) {
      const nextName = splitName(profile.name);
      firstName.value = nextName.first;
      lastName.value = nextName.last;
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
  resetLoginFlow();
  timer = window.setInterval(() => {
    currentSlide.value = (currentSlide.value + 1) % slides.length;
  }, 5000);
});

onBeforeUnmount(() => {
  if (timer) {
    window.clearInterval(timer);
  }
});

function relayLooksValid(value: string) {
  const candidate = normalizeRelayLikeValue(value);
  if (!candidate) {
    return false;
  }

  try {
    const parsed = new URL(candidate);
    return (parsed.protocol === "ws:" || parsed.protocol === "wss:") && !!parsed.hostname;
  } catch {
    return false;
  }
}

function relayQueryLooksValid(value: string) {
  try {
    const relay = new URL(value);
    return (relay.protocol === "ws:" || relay.protocol === "wss:") && !!relay.hostname;
  } catch {
    return false;
  }
}

function splitName(value: string) {
  const tokens = value.trim().split(/\s+/).filter(Boolean);
  return {
    first: tokens[0] ?? "",
    last: tokens.slice(1).join(" "),
  };
}

function normalizeRelayLikeValue(value: string) {
  const trimmed = value.trim();
  if (!trimmed) {
    return "";
  }

  return trimmed.includes("://") ? trimmed : `wss://${trimmed}`;
}

function defaultCircleModeForMethod(method: LoginMethod): LoginCircleSelectionMode {
  if (method === "quickStart") {
    return "invite";
  }

  return props.restorableCircles.length > 0 ? "restore" : "invite";
}

function resetLoginFlow() {
  const nextName = splitName(props.profile.name);
  currentSlide.value = 0;
  currentStep.value = 0;
  selectedMethod.value = "quickStart";
  accountKey.value = "";
  handle.value = props.profile.handle;
  profileStatus.value = props.profile.status;
  circleMode.value = defaultCircleModeForMethod("quickStart");
  selectedCircleId.value = props.circles[0]?.id ?? "";
  selectedRestoreRelay.value = props.restorableCircles[0]?.relay ?? "";
  inviteCode.value = "";
  inviteName.value = "";
  customCircleName.value = "";
  customRelay.value = "";
  activeCircleSheet.value = null;
  activeInfoSheet.value = null;
  firstName.value = nextName.first;
  lastName.value = nextName.last;
}

function deriveAccountKeyAccessKind(value: string): LoginAccessInput["kind"] | null {
  const trimmed = value.trim();
  const lowered = trimmed.toLowerCase();
  if (!trimmed) {
    return null;
  }

  if (lowered.startsWith("nsec")) {
    return NSEC_PATTERN.test(trimmed) ? "nsec" : null;
  }

  if (lowered.startsWith("npub")) {
    return NPUB_PATTERN.test(trimmed) ? "npub" : null;
  }

  if (lowered.startsWith("bunker://")) {
    return remoteSignerUriLooksValid(trimmed, "bunker", false) ? "bunker" : null;
  }

  if (lowered.startsWith("nostrconnect://")) {
    return remoteSignerUriLooksValid(trimmed, "nostrconnect", true) ? "nostrConnect" : null;
  }

  return HEX_KEY_PATTERN.test(trimmed) ? "hexKey" : null;
}

function remoteSignerUriLooksValid(value: string, scheme: "bunker" | "nostrconnect", requireSecret: boolean) {
  try {
    const uri = new URL(value);
    if (uri.protocol.toLowerCase() !== `${scheme}:`) {
      return false;
    }

    if (!HEX_PUBKEY_PATTERN.test(uri.hostname)) {
      return false;
    }

    const relays = uri.searchParams.getAll("relay").filter(relayQueryLooksValid);
    if (!relays.length) {
      return false;
    }

    if (requireSecret && !uri.searchParams.get("secret")?.trim()) {
      return false;
    }

    return true;
  } catch {
    return false;
  }
}

function openQuickStart() {
  selectedMethod.value = "quickStart";
  currentStep.value = 2;
  circleMode.value = defaultCircleModeForMethod("quickStart");
  activeCircleSheet.value = null;
  activeInfoSheet.value = null;
}

function openExistingAccount() {
  selectedMethod.value = "existingAccount";
  currentStep.value = 1;
  circleMode.value = defaultCircleModeForMethod("existingAccount");
  activeCircleSheet.value = null;
  activeInfoSheet.value = null;
}

function goBack() {
  activeCircleSheet.value = null;
  activeInfoSheet.value = null;
  if (currentStep.value === 1 || currentStep.value === 2) {
    currentStep.value = 0;
    return;
  }

  currentStep.value = selectedMethod.value === "quickStart" ? 2 : 1;
}

function handlePrimaryAction() {
  if (!stepValid.value) {
    return;
  }

  if (currentStep.value === 1) {
    if (needsCircleSelectionStep.value) {
      currentStep.value = 3;
      return;
    }

    submit();
    return;
  }

  if (currentStep.value === 2) {
    currentStep.value = 3;
    return;
  }

  handleCircleConnect();
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

function buildCustomCircleName() {
  const trimmed = customCircleName.value.trim();
  if (trimmed) {
    return trimmed;
  }

  const relayLabel = customRelay.value
    .trim()
    .replace(/^wss?:\/\//i, "")
    .split(/[/?#]/)[0]
    ?.trim();

  return relayLabel || "Custom Relay";
}

function archivedAtCopy(value: string) {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }

  return parsed.toLocaleString();
}

function buildCircleMeta(circle: CircleItem | RestorableCircleEntry) {
  return `${circle.type} circle`;
}

function selectCircleMode(mode: LoginCircleSelectionMode) {
  circleMode.value = mode;
  if (mode !== "invite" && mode !== "custom") {
    activeCircleSheet.value = null;
  }
}

function openCircleSheet(mode: CircleSheetMode) {
  circleMode.value = mode;
  activeCircleSheet.value = mode;
}

function closeCircleSheet() {
  activeCircleSheet.value = null;
}

function openInfoSheet(kind: InfoSheetKind) {
  activeInfoSheet.value = kind;
}

function closeInfoSheet() {
  activeInfoSheet.value = null;
}

function applyRelaySuggestion(value: string) {
  customRelay.value = value;
}

function handleCircleConnect() {
  if (circleMode.value === "restore" || circleMode.value === "existing") {
    submit();
    return;
  }

  if (circleMode.value === "invite") {
    if (inviteCode.value.trim().length < 6) {
      openCircleSheet("invite");
      return;
    }

    submit();
    return;
  }

  if (!relayLooksValid(customRelay.value)) {
    openCircleSheet("custom");
    return;
  }

  submit();
}

function confirmCircleSheet() {
  if (!circleSheetValid.value || !activeCircleSheet.value) {
    return;
  }

  activeCircleSheet.value = null;
  submit();
}

function deriveSubmittedMethod(accessKind: LoginAccessInput["kind"]): LoginMethod {
  if (selectedMethod.value === "existingAccount" && (accessKind === "bunker" || accessKind === "nostrConnect")) {
    return "signer";
  }

  return selectedMethod.value;
}

function submit() {
  if (!stepValid.value) {
    return;
  }

  const resolvedAccessKind = accountKeyAccessKind.value;
  const access: LoginAccessInput =
    selectedMethod.value === "quickStart"
      ? {
          kind: "localProfile",
        }
      : {
          kind: resolvedAccessKind ?? "hexKey",
          value: normalizedAccountKey.value,
        };
  const method = deriveSubmittedMethod(access.kind);

  const payload: LoginCompletionInput = {
    method,
    access,
    userProfile: {
      name: displayName.value || props.profile.name || "XChat User",
      handle: normalizedHandle.value,
      initials: buildInitials(displayName.value || props.profile.name),
      status: profileStatus.value.trim() || "Circle member",
    },
    circleSelection:
      selectedMethod.value === "existingAccount" && canReuseExistingCircleImmediately.value
        ? {
            mode: "existing",
            circleId: selectedCircleId.value || props.circles[0]?.id || "",
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
                  name: buildCustomCircleName(),
                  relay: customRelay.value.trim(),
                },
  };

  emit("complete", payload);
}
</script>

<template>
  <section class="login-screen">
    <div v-if="currentStep === 0" class="entry-shell">
      <div class="entry-carousel">
        <div class="entry-art-wrap">
          <img :src="slides[currentSlide].image" :alt="slides[currentSlide].title" class="entry-art" />
        </div>

        <div class="entry-copy">
          <h1>{{ slides[currentSlide].title }}</h1>
          <p>{{ slides[currentSlide].text }}</p>
        </div>

        <div class="slide-dots">
          <button
            v-for="(_, index) in slides"
            :key="index"
            type="button"
            :class="['slide-dot', { active: index === currentSlide }]"
            :aria-label="`Open slide ${index + 1}`"
            @click="currentSlide = index"
          />
        </div>
      </div>

      <div class="entry-footer">
        <button type="button" class="action-button action-primary" @click="openQuickStart">
          Get Started
        </button>
        <button type="button" class="action-button action-secondary" @click="openExistingAccount">
          I have a Nostr account
        </button>

        <div class="terms-row">
          <span class="terms-check" aria-hidden="true"></span>
          <p>
            By continuing, you agree to our
            <span class="terms-link">Privacy Policy</span>
            and
            <span class="terms-link">Terms of Use</span>
          </p>
        </div>
      </div>
    </div>

    <div v-else class="mobile-page">
      <header class="mobile-topbar">
        <button type="button" class="topbar-back" aria-label="Go back" @click="goBack">
          <span class="topbar-chevron">‹</span>
        </button>
        <h1 class="topbar-title">{{ currentStep === 1 ? "LOGIN" : "" }}</h1>
        <span class="topbar-side" aria-hidden="true"></span>
      </header>

      <div class="mobile-body">
        <template v-if="currentStep === 1">
          <section class="page-section auth-section">
            <h2 class="page-title page-title-left">Use Nostr key or remote signer to login:</h2>

            <Textarea
              id="account-key"
              v-model="accountKey"
              rows="4"
              auto-resize
              class="wide-input"
              placeholder="Enter nsec, npub, hex, or signer URI"
            />

            <p class="section-footnote">
              Enter your nostr private key or remote signer URI link.
              <button type="button" class="inline-link-button" @click="openInfoSheet('nostr')">Learn more</button>
            </p>

            <p v-if="invalidNsecHintVisible" class="inline-hint tone-error">
              The NSEC you entered is invalid. Please enter it again.
            </p>
            <p v-else-if="invalidRemoteSignerHintVisible" class="inline-hint tone-error">
              Signer URI must include a 64-character pubkey and at least one <code>ws://</code> or <code>wss://</code> relay. <code>nostrconnect://</code> also requires a secret.
            </p>
            <p v-else-if="npubSelected" class="inline-hint tone-warn">
              <code>npub</code> import is read-only and cannot send messages yet.
            </p>
            <p v-else-if="nostrConnectSelected" class="inline-hint tone-warn">
              <code>nostrconnect://</code> login can bootstrap signer state, but desktop send is still blocked on the local client-URI flow.
            </p>
            <p v-else-if="remoteSignerSelected" class="inline-hint tone-info">
              Remote signer login will continue through signer bootstrap after you tap LOGIN.
            </p>
            <p v-else-if="canReuseExistingCircleImmediately" class="inline-hint tone-info">
              Saved circles already exist in this shell. Login will return directly to the current circle.
            </p>
          </section>
        </template>

        <template v-else-if="currentStep === 2">
          <section class="page-section profile-section">
            <div class="page-copy centered-copy">
              <h2 class="page-title">Create Your Profile</h2>
              <p class="page-subtitle">
                Enter your name to get started. Your nostr account is being created automatically.
              </p>
            </div>

            <button type="button" class="avatar-preview" aria-label="Profile preview">
              <span class="profile-avatar">{{ buildInitials(displayName || "XC") }}</span>
              <span class="avatar-badge">
                <i class="pi pi-camera" />
              </span>
            </button>

            <div class="input-stack">
              <InputText id="first-name" v-model="firstName" class="wide-input" placeholder="First Name" />
              <InputText id="last-name" v-model="lastName" class="wide-input" placeholder="Last Name (Optional)" />
            </div>

            <p class="section-footnote">This is how others will see you. You can change this anytime.</p>
            <p class="inline-hint tone-muted">Username will be saved as {{ normalizedHandle }}</p>
          </section>
        </template>

        <template v-else>
          <section class="page-section circle-section">
            <div class="page-copy centered-copy">
              <h2 class="page-title">Add Circle</h2>
              <p class="page-subtitle">Connect to your circle using an invite link or choose a relay provider.</p>
            </div>

            <div class="option-stack">
              <button
                type="button"
                :class="['option-card', { active: circleMode === 'invite' }]"
                @click="selectCircleMode('invite')"
              >
                <span class="option-icon">
                  <i class="pi pi-link" />
                </span>
                <span class="option-copy">
                  <strong>I have an invite</strong>
                  <small>Scan QR code or enter invitation link</small>
                </span>
                <i class="pi pi-angle-right option-arrow" />
              </button>

              <div class="option-divider">
                <span></span>
                <p>OR CONNECT VIA</p>
                <span></span>
              </div>

              <div class="recommended-card">
                <span class="recommended-badge">RECOMMENDED</span>
                <button type="button" class="option-card option-card-disabled" disabled>
                  <span class="option-icon">
                    <i class="pi pi-diamond" />
                  </span>
                  <span class="option-copy">
                    <strong>Get Private Circle</strong>
                    <small>Hosted dedicated high-speed relay with built-in secure media server</small>
                  </span>
                </button>
              </div>

              <button
                type="button"
                :class="['option-card', { active: circleMode === 'custom' }]"
                @click="selectCircleMode('custom')"
              >
                <span class="option-icon">
                  <i class="pi pi-server" />
                </span>
                <span class="option-copy">
                  <strong>Custom Relay</strong>
                  <small>Community or Self-hosted nodes</small>
                </span>
                <i class="pi pi-angle-right option-arrow" />
              </button>
            </div>

            <p class="inline-hint tone-muted">
              Private Circle stays paused in this desktop rebuild because it depends on backend services.
            </p>

            <button
              type="button"
              :class="['restore-link', { active: circleMode === 'restore', disabled: !canRestoreCirclesAfterLogin }]"
              :disabled="!canRestoreCirclesAfterLogin"
              @click="selectCircleMode('restore')"
            >
              Restore private circle
            </button>

            <div v-if="circleMode === 'restore'" class="detail-panel">
              <div v-if="props.restorableCircles.length" class="selection-list">
                <button
                  v-for="circle in props.restorableCircles"
                  :key="circle.relay"
                  type="button"
                  :class="['selection-card', { active: selectedRestoreRelay === circle.relay }]"
                  @click="selectedRestoreRelay = circle.relay"
                >
                  <div class="selection-head">
                    <strong>{{ circle.name }}</strong>
                    <span class="selection-pill">{{ buildCircleMeta(circle) }}</span>
                  </div>
                  <p>{{ circle.description || "No archived description available." }}</p>
                  <span class="selection-meta">{{ circle.relay }}</span>
                  <span class="selection-meta">Archived {{ archivedAtCopy(circle.archivedAt) }}</span>
                </button>
              </div>
              <p v-else class="inline-hint tone-muted">No private circle is available to restore right now.</p>
            </div>
          </section>
        </template>
      </div>

      <footer class="mobile-footer">
        <button type="button" class="action-button action-dark" :disabled="!stepValid" @click="handlePrimaryAction">
          {{ primaryActionLabel }}
        </button>
      </footer>
    </div>

    <Transition name="sheet-fade">
      <div v-if="activeCircleSheet" class="circle-sheet-layer" @click.self="closeCircleSheet">
        <div class="circle-sheet-card" role="dialog" aria-modal="true">
          <span class="circle-sheet-grabber" aria-hidden="true"></span>

          <div class="circle-sheet-copy">
            <h3>{{ activeCircleSheet === "invite" ? "Enter Invitation Link" : "Add Relay" }}</h3>
            <p v-if="activeCircleSheet === 'invite'">
              Paste your invite link or invitation code, then continue into the selected circle.
            </p>
            <p v-else>
              Enter the relay address to join.
              <button type="button" class="inline-link-button" @click="openInfoSheet('relay')">What is a Relay?</button>
            </p>
          </div>

          <div class="circle-sheet-body">
            <template v-if="activeCircleSheet === 'invite'">
              <label class="field-label" for="invite-code-sheet">Invitation Link</label>
              <InputText
                id="invite-code-sheet"
                v-model="inviteCode"
                class="wide-input"
                placeholder="circle://..., invite://..., or invitation code"
              />
            </template>

            <template v-else>
              <label class="field-label" for="custom-relay-sheet">Relay URL or name</label>
              <InputText
                id="custom-relay-sheet"
                v-model="customRelay"
                class="wide-input"
                placeholder="wss://relay.example.com"
              />

              <div class="relay-suggestions">
                <button type="button" class="relay-chip" @click="applyRelaySuggestion('0xchat')">0xchat</button>
                <button type="button" class="relay-chip" @click="applyRelaySuggestion('damus')">damus</button>
              </div>

              <p class="section-footnote sheet-footnote">
                You can enter a full URL like <code>wss://relay.example.com</code> or use a shortcut relay name
                such as <code>0xchat</code> or <code>damus</code>.
              </p>

              <p v-if="normalizedCustomRelayPreview" class="inline-hint tone-muted">
                Relay will be saved as {{ normalizedCustomRelayPreview }}
              </p>
            </template>
          </div>

          <div class="circle-sheet-actions">
            <button type="button" class="sheet-button sheet-button-secondary" @click="closeCircleSheet">Cancel</button>
            <button
              type="button"
              class="sheet-button sheet-button-primary"
              :disabled="!circleSheetValid"
              @click="confirmCircleSheet"
            >
              Connect
            </button>
          </div>
        </div>
      </div>
    </Transition>

    <Transition name="sheet-fade">
      <div v-if="activeInfoSheet" class="circle-sheet-layer" @click.self="closeInfoSheet">
        <div class="circle-sheet-card info-sheet-card" role="dialog" aria-modal="true">
          <span class="circle-sheet-grabber" aria-hidden="true"></span>

          <div class="circle-sheet-copy">
            <h3>{{ activeInfoSheet === "nostr" ? "Understanding Nostr" : "What is a Circle?" }}</h3>
            <p v-if="activeInfoSheet === 'nostr'">
              Learn about Nostr keys, signer URIs, and the safest way to authenticate on this device.
            </p>
            <p v-else>
              Your circle is the relay hub that stores encrypted messages until you and your contacts sync them.
            </p>
          </div>

          <div class="circle-sheet-body info-sheet-body">
            <template v-if="activeInfoSheet === 'nostr'">
              <article class="info-card">
                <h4>What is Nostr?</h4>
                <p>
                  Nostr is a decentralized protocol that uses cryptographic keys for identity, signing, and message
                  exchange without a central account server.
                </p>
              </article>

              <article class="info-card">
                <h4>Private keys and <code>nsec</code></h4>
                <p>
                  Your private key proves your identity and signs outgoing events. A normal login key usually starts
                  with <code>nsec1...</code>, while this desktop rebuild also accepts raw 64-character hex keys.
                </p>
              </article>

              <article class="info-card">
                <h4>Remote signer</h4>
                <p>
                  A remote signer keeps the private key off-device and signs on your behalf through
                  <code>bunker://</code> or <code>nostrconnect://</code> links.
                </p>
              </article>

              <article class="info-card">
                <h4>How to use it</h4>
                <ol class="info-step-list">
                  <li class="info-step-item">Enter your <code>nsec</code> or 64-character hex key for direct login.</li>
                  <li class="info-step-item">Or paste a valid <code>bunker://</code> or <code>nostrconnect://</code> signer URI.</li>
                  <li class="info-step-item">The app will authenticate and continue into profile or circle setup.</li>
                </ol>
              </article>
            </template>

            <template v-else>
              <article class="info-card">
                <h4>Your private communication hub</h4>
                <p>
                  A circle is the relay address your chats use for delivery, storage, and sync. Messages stay
                  encrypted, so the relay moves data without being able to read it.
                </p>
              </article>

              <article class="info-card">
                <h4>Think of it like a mailroom</h4>
                <p>The relay receives messages, holds them until recipients sync, and forwards them to the right contacts.</p>
                <p>Community circles are shared public relays. Private circles are dedicated relays with more control.</p>
              </article>

              <article class="info-card">
                <h4>How it works</h4>
                <ol class="info-step-list">
                  <li class="info-step-item">You send a message to the circle relay.</li>
                  <li class="info-step-item">The relay stores the encrypted event until recipients fetch it.</li>
                  <li class="info-step-item">Your contacts sync from the same relay and decrypt locally.</li>
                  <li class="info-step-item">You can enter a full <code>wss://</code> URL or a shortcut like <code>0xchat</code>.</li>
                </ol>
              </article>
            </template>
          </div>

          <div class="circle-sheet-actions sheet-button-single">
            <button type="button" class="sheet-button sheet-button-primary" @click="closeInfoSheet">Close</button>
          </div>
        </div>
      </div>
    </Transition>
  </section>
</template>

<style scoped>
.login-screen {
  min-height: calc(100vh - 24px);
  display: grid;
  place-items: center;
  padding: 12px;
}

.entry-shell {
  width: min(430px, calc(100vw - 32px));
  min-height: min(820px, calc(100vh - 32px));
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  color: #f6f8ff;
  padding: 20px 32px 16px;
}

.entry-carousel,
.entry-footer,
.mobile-page,
.input-stack,
.selection-list,
.option-stack,
.detail-panel,
.page-section {
  display: grid;
}

.entry-carousel {
  gap: 14px;
  justify-items: center;
  align-self: center;
  width: 100%;
  margin-top: auto;
  margin-bottom: auto;
}

.entry-art-wrap {
  width: 100%;
  min-height: 300px;
  display: grid;
  place-items: center;
}

.entry-art {
  width: min(280px, 65vw);
  max-width: 280px;
  filter: drop-shadow(0 22px 52px rgba(10, 24, 48, 0.16));
}

.entry-copy {
  display: grid;
  gap: 10px;
  text-align: center;
}

.entry-copy h1,
.entry-copy p,
.page-title,
.page-subtitle,
.section-footnote,
.inline-hint,
.selection-card p,
.restore-link {
  margin: 0;
}

.entry-copy h1 {
  font-size: clamp(1.55rem, 3vw, 1.8rem);
  line-height: 1.2;
  letter-spacing: -0.04em;
  font-weight: 700;
}

.entry-copy p {
  max-width: 30ch;
  line-height: 1.55;
  color: rgba(246, 248, 255, 0.8);
}

.slide-dots {
  display: flex;
  gap: 6px;
  align-items: center;
}

.slide-dot {
  width: 8px;
  height: 8px;
  padding: 0;
  border: 0;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.28);
  cursor: pointer;
}

.slide-dot.active {
  width: 22px;
  background: #ffffff;
}

.entry-footer {
  gap: 16px;
}

.action-button {
  width: 100%;
  min-height: 48px;
  border: 0;
  border-radius: 999px;
  font-size: 1rem;
  font-weight: 600;
  cursor: pointer;
  transition:
    transform 160ms ease,
    box-shadow 160ms ease,
    opacity 160ms ease;
}

.action-button:hover:enabled {
  transform: translateY(-1px);
}

.action-button:disabled {
  cursor: not-allowed;
  opacity: 0.52;
}

.action-primary {
  background: #ffffff;
  color: #214481;
  box-shadow: 0 16px 40px rgba(9, 28, 56, 0.2);
}

.action-secondary {
  background: transparent;
  color: #ffffff;
}

.action-dark {
  background: #0f1729;
  color: #ffffff;
}

.terms-row {
  display: grid;
  grid-template-columns: 18px minmax(0, 1fr);
  gap: 10px;
  align-items: start;
  color: rgba(246, 248, 255, 0.72);
  font-size: 0.8rem;
  line-height: 1.6;
}

.terms-row p {
  margin: 0;
}

.terms-link {
  text-decoration: underline;
  text-underline-offset: 0.14em;
}

.terms-check {
  display: grid;
  place-items: center;
  width: 16px;
  height: 16px;
  border: 1px solid rgba(255, 255, 255, 0.35);
  border-radius: 4px;
  margin-top: 2px;
}

.terms-check::after {
  content: "";
  width: 9px;
  height: 9px;
  border-radius: 2px;
  background: rgba(255, 255, 255, 0.9);
}

.mobile-page {
  width: min(430px, calc(100vw - 24px));
  min-height: min(820px, calc(100vh - 24px));
  background: #ffffff;
  border-radius: 0;
  border: 0;
  box-shadow: none;
  overflow: hidden;
  grid-template-rows: auto minmax(0, 1fr) auto;
}

.mobile-topbar,
.selection-head,
.option-card {
  display: flex;
  align-items: center;
}

.mobile-topbar {
  grid-template-columns: 44px minmax(0, 1fr) 44px;
  display: grid;
  align-items: center;
  padding: 16px 18px 10px;
}

.topbar-back {
  width: 40px;
  height: 40px;
  display: grid;
  place-items: center;
  border: 0;
  background: transparent;
  cursor: pointer;
}

.topbar-chevron {
  font-size: 1.7rem;
  line-height: 1;
  color: #5f728d;
  transform: translateY(-1px);
}

.topbar-title {
  min-height: 24px;
  text-align: center;
  font-size: 0.95rem;
  font-weight: 700;
  letter-spacing: 0.12em;
  color: #223553;
}

.topbar-side {
  width: 40px;
  height: 40px;
}

.mobile-body {
  overflow: auto;
  padding: 10px 30px 24px;
}

.page-section {
  gap: 18px;
}

.page-copy {
  display: grid;
  gap: 12px;
}

.centered-copy {
  justify-items: center;
  text-align: center;
}

.page-title {
  color: #20324f;
  font-size: 1.95rem;
  line-height: 1.08;
  letter-spacing: -0.04em;
  font-weight: 700;
}

.page-title-left {
  font-size: 1.45rem;
  line-height: 1.24;
}

.page-subtitle,
.section-footnote {
  color: #7b8ca5;
  line-height: 1.6;
  font-size: 0.94rem;
}

.inline-link {
  color: #2b6fce;
}

.inline-link-button {
  padding: 0;
  border: 0;
  background: transparent;
  color: #2b6fce;
  cursor: pointer;
  font: inherit;
  font-weight: 600;
  text-decoration: underline;
  text-underline-offset: 0.16em;
}

.avatar-preview {
  width: 88px;
  height: 88px;
  margin: 4px auto 0;
  padding: 0;
  border: 0;
  background: transparent;
  position: relative;
  cursor: default;
}

.profile-avatar {
  width: 80px;
  height: 80px;
  display: grid;
  place-items: center;
  border-radius: 999px;
  background: linear-gradient(135deg, #dbe9ff 0%, #ddf8ef 100%);
  color: #17345c;
  font-size: 1.35rem;
  font-weight: 700;
  margin: 0 auto;
}

.avatar-badge {
  position: absolute;
  right: 0;
  bottom: 0;
  width: 24px;
  height: 24px;
  display: grid;
  place-items: center;
  border-radius: 999px;
  background: #224481;
  color: #ffffff;
  border: 2px solid #ffffff;
  font-size: 0.72rem;
}

.input-stack,
.detail-panel,
.selection-list {
  gap: 12px;
}

.field-label {
  color: #485b75;
  font-size: 0.86rem;
  font-weight: 600;
}

.inline-hint {
  font-size: 0.9rem;
  line-height: 1.55;
}

.tone-error {
  color: #c95353;
}

.tone-warn {
  color: #9a6820;
}

.tone-info {
  color: #4f6e98;
}

.tone-muted {
  color: #7b8ca5;
}

.option-stack {
  gap: 16px;
}

.option-card {
  width: 100%;
  gap: 16px;
  padding: 18px 20px;
  border-radius: 18px;
  border: 1px solid rgba(210, 221, 233, 0.86);
  background: #f8fbff;
  text-align: left;
  cursor: pointer;
}

.option-card.active {
  border-color: #6fa0ea;
  box-shadow: inset 0 0 0 1px rgba(111, 160, 234, 0.3);
}

.option-card-disabled {
  cursor: not-allowed;
  opacity: 0.56;
}

.option-icon {
  display: grid;
  place-items: center;
  width: 32px;
  height: 32px;
  border-radius: 12px;
  color: #2d6bd0;
  font-size: 1.05rem;
}

.option-copy {
  display: grid;
  gap: 4px;
  flex: 1;
}

.option-copy strong {
  color: #21324f;
  font-size: 1rem;
  font-weight: 700;
}

.option-copy small {
  color: #73859d;
  font-size: 0.87rem;
  line-height: 1.45;
}

.option-arrow {
  color: #8ba0bc;
  font-size: 0.92rem;
}

.option-divider {
  display: grid;
  grid-template-columns: 1fr auto 1fr;
  gap: 14px;
  align-items: center;
}

.option-divider span {
  display: block;
  height: 1px;
  background: rgba(144, 162, 187, 0.22);
}

.option-divider p {
  margin: 0;
  color: #91a0b7;
  font-size: 0.75rem;
  font-weight: 700;
  letter-spacing: 0.12em;
}

.recommended-card {
  position: relative;
}

.recommended-badge {
  position: absolute;
  top: -8px;
  right: 10px;
  z-index: 1;
  padding: 4px 8px;
  border-radius: 6px;
  background: #214481;
  color: #ffffff;
  font-size: 0.68rem;
  font-weight: 700;
  letter-spacing: 0.06em;
}

.restore-link {
  padding: 0;
  border: 0;
  background: transparent;
  color: #2b6fce;
  font-size: 0.95rem;
  justify-self: center;
  cursor: pointer;
}

.restore-link.disabled {
  cursor: not-allowed;
  opacity: 0.45;
}

.restore-link.active {
  color: #1b56a7;
}

.wide-input {
  width: 100%;
}

.selection-card {
  width: 100%;
  border: 1px solid rgba(211, 221, 232, 0.95);
  border-radius: 16px;
  background: #ffffff;
  text-align: left;
  cursor: pointer;
}

.selection-card.active {
  border-color: rgba(83, 132, 193, 0.82);
  background: linear-gradient(180deg, #f8fbff 0%, #f6fbf8 100%);
}

.selection-card {
  display: grid;
  gap: 10px;
  padding: 16px;
}

.selection-head {
  justify-content: space-between;
  gap: 12px;
}

.selection-head strong {
  color: #20324f;
}

.selection-meta,
.selection-card p {
  color: #6d8098;
}

.selection-meta {
  font-size: 0.84rem;
}

.selection-card p,
.summary-card p {
  line-height: 1.6;
}

.selection-pill {
  padding: 6px 10px;
  border-radius: 999px;
  background: rgba(41, 87, 153, 0.08);
  color: #44628c;
  font-size: 0.72rem;
  text-transform: capitalize;
}

.mobile-footer {
  padding: 16px 30px 24px;
  background: linear-gradient(180deg, rgba(255, 255, 255, 0) 0%, #ffffff 24%);
}

.circle-sheet-layer {
  position: fixed;
  inset: 0;
  display: grid;
  align-items: end;
  background: rgba(15, 23, 41, 0.36);
  padding: 16px;
}

.circle-sheet-card,
.circle-sheet-copy,
.circle-sheet-body,
.circle-sheet-actions,
.relay-suggestions {
  display: grid;
}

.circle-sheet-card {
  width: min(430px, calc(100vw - 32px));
  margin: 0 auto;
  gap: 18px;
  padding: 14px 20px 20px;
  border-radius: 28px;
  background: #ffffff;
  box-shadow: 0 24px 64px rgba(15, 23, 41, 0.18);
}

.circle-sheet-grabber {
  width: 44px;
  height: 5px;
  justify-self: center;
  border-radius: 999px;
  background: rgba(124, 140, 164, 0.34);
}

.circle-sheet-copy {
  gap: 8px;
}

.circle-sheet-copy h3,
.circle-sheet-copy p,
.circle-sheet-actions {
  margin: 0;
}

.circle-sheet-copy h3 {
  color: #20324f;
  font-size: 1.2rem;
  line-height: 1.2;
  font-weight: 700;
}

.circle-sheet-copy p {
  color: #73859d;
  line-height: 1.6;
}

.circle-sheet-body {
  gap: 12px;
}

.sheet-footnote {
  margin: 0;
}

.relay-suggestions {
  grid-auto-flow: column;
  grid-auto-columns: max-content;
  gap: 8px;
}

.relay-chip,
.sheet-button {
  border: 0;
  cursor: pointer;
  font: inherit;
}

.relay-chip {
  padding: 8px 12px;
  border-radius: 999px;
  background: #eef4fb;
  color: #315279;
  font-size: 0.9rem;
  font-weight: 600;
}

.circle-sheet-actions {
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}

.sheet-button {
  min-height: 46px;
  border-radius: 999px;
  font-weight: 700;
}

.sheet-button:disabled {
  cursor: not-allowed;
  opacity: 0.5;
}

.sheet-button-secondary {
  background: #eef4fb;
  color: #315279;
}

.sheet-button-primary {
  background: #0f1729;
  color: #ffffff;
}

.info-sheet-card {
  gap: 20px;
}

.info-sheet-body {
  gap: 14px;
}

.info-card {
  display: grid;
  gap: 8px;
  padding: 16px;
  border-radius: 18px;
  border: 1px solid rgba(210, 221, 233, 0.82);
  background: linear-gradient(180deg, #f9fbff 0%, #f4f8fd 100%);
}

.info-card h4,
.info-card p,
.info-step-list {
  margin: 0;
}

.info-card h4 {
  color: #20324f;
  font-size: 0.98rem;
  line-height: 1.35;
  font-weight: 700;
}

.info-card p,
.info-step-list {
  color: #667a94;
  line-height: 1.6;
  font-size: 0.92rem;
}

.info-step-list {
  display: grid;
  gap: 10px;
  padding-left: 1.2rem;
}

.sheet-button-single {
  grid-template-columns: 1fr;
}

.sheet-fade-enter-active,
.sheet-fade-leave-active {
  transition: opacity 0.18s ease;
}

.sheet-fade-enter-active .circle-sheet-card,
.sheet-fade-leave-active .circle-sheet-card {
  transition:
    transform 0.18s ease,
    opacity 0.18s ease;
}

.sheet-fade-enter-from,
.sheet-fade-leave-to {
  opacity: 0;
}

.sheet-fade-enter-from .circle-sheet-card,
.sheet-fade-leave-to .circle-sheet-card {
  transform: translateY(16px);
  opacity: 0.96;
}

:deep(.p-inputtext),
:deep(.p-inputtextarea) {
  width: 100%;
  border-radius: 16px;
  border-color: rgba(208, 218, 228, 0.95);
  box-shadow: none;
  padding: 0.95rem 1rem;
  font-size: 0.98rem;
  color: #20324f;
}

:deep(.p-inputtext:enabled:focus),
:deep(.p-inputtextarea:enabled:focus) {
  border-color: #5d8dd6;
  box-shadow: 0 0 0 4px rgba(93, 141, 214, 0.12);
}

code {
  font-family: inherit;
  font-size: 0.92em;
  padding: 0 0.22em;
  border-radius: 6px;
  background: rgba(16, 24, 40, 0.08);
}

@media (max-width: 720px) {
  .entry-shell,
  .mobile-page {
    width: calc(100vw - 16px);
    min-height: calc(100vh - 16px);
  }

  .entry-shell {
    padding-left: 24px;
    padding-right: 24px;
  }

  .mobile-body,
  .mobile-footer {
    padding-left: 20px;
    padding-right: 20px;
  }

  .circle-sheet-layer {
    padding: 10px;
  }

  .circle-sheet-card {
    width: calc(100vw - 20px);
  }
}
</style>
