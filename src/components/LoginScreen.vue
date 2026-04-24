<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import InputText from "primevue/inputtext";
import Textarea from "primevue/textarea";
import onboardingWelcomeImage from "../assets/onboarding-welcome.svg";
import onboardingNostrImage from "../assets/onboarding-nostr.svg";
import onboardingCircleImage from "../assets/onboarding-circle.svg";
import onboardingRelaysImage from "../assets/onboarding-relays.svg";
import { bootstrapAuthSession } from "../services/chatShell";
import type {
  CircleItem,
  LoginAccessInput,
  LoginCircleSelectionMode,
  LoginCompletionInput,
  LoginMethod,
  RestorableCircleEntry,
  UserProfile,
} from "../types/chat";

type CircleSelectionMode = LoginCircleSelectionMode | "privatePreview";
type CircleSheetMode = "custom";
type CirclePageKind =
  | "restore"
  | "privateOverview"
  | "privateLearnMore"
  | "privateCapacity"
  | "privateDuration"
  | "privateCheckout"
  | "privateActivated";
type InfoSheetKind = "nostr" | "relay";
type PrivatePlanId = "lovers" | "family" | "community";
type PrivateBillingPeriod = "yearly" | "monthly";

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
const HEX_KEY_PATTERN = /^(?:0x)?[a-f0-9]{64}$/i;
const PUBLIC_RELAY_SHORTCUTS = {
  "0xchat": "wss://relay.0xchat.com",
  damus: "wss://relay.damus.io",
  nos: "wss://nos.lol",
  primal: "wss://relay.primal.net",
  yabu: "wss://yabu.me",
  nostrband: "wss://relay.nostr.band",
} as const;
const PRIVATE_PROGRESS_LABELS = ["CAPACITY", "DURATION", "CHECKOUT"] as const;
const PRIVATE_PLAN_OPTIONS = [
  {
    id: "lovers",
    title: "≤ 2 Members",
    planName: "2 Members",
    maxUsers: 2,
    description: "For you and your partner.",
    monthlyPrice: "$4.99",
    yearlyPrice: "$49.99",
    accent: "rose",
    mostPopular: false,
  },
  {
    id: "family",
    title: "≤ 6 Members",
    planName: "6 Members",
    maxUsers: 6,
    description: "Complete privacy for home.",
    monthlyPrice: "$9.99",
    yearlyPrice: "$99.99",
    accent: "blue",
    mostPopular: true,
  },
  {
    id: "community",
    title: "≤ 20 Members",
    planName: "20 Members",
    maxUsers: 20,
    description: "For groups & creators.",
    monthlyPrice: "$24.99",
    yearlyPrice: "$249.99",
    accent: "lavender",
    mostPopular: false,
  },
] as const;

function hasTauriRuntime() {
  const globalWindow = globalThis as typeof globalThis & {
    __TAURI__?: unknown;
    __TAURI_INTERNALS__?: unknown;
  };

  return typeof window !== "undefined" && ("__TAURI_INTERNALS__" in globalWindow || "__TAURI__" in globalWindow);
}

const currentSlide = ref(0);
const currentStep = ref(0);
const selectedMethod = ref<LoginMethod>("quickStart");
const accountKey = ref("");
const handle = ref(props.profile.handle);
const profileStatus = ref(props.profile.status);
const circleMode = ref<CircleSelectionMode | null>(defaultCircleModeForMethod("quickStart"));
const selectedCircleId = ref(props.circles[0]?.id ?? "");
const selectedRestoreRelays = ref(props.restorableCircles.map((circle) => circle.relay));
const customCircleName = ref("");
const customRelay = ref("");
const activeCircleSheet = ref<CircleSheetMode | null>(null);
const activeCirclePage = ref<CirclePageKind | null>(null);
const activeInfoSheet = ref<InfoSheetKind | null>(null);
const avatarInput = ref<HTMLInputElement | null>(null);
const avatarPreviewUrl = ref("");
const privateCircleName = ref("My Private Circle");
const selectedPrivatePlanId = ref<PrivatePlanId>("family");
const selectedPrivateBilling = ref<PrivateBillingPeriod>("yearly");
const loginPreparationBusy = ref(false);
const loginPreparationError = ref("");
const isNativeDesktopRuntime = hasTauriRuntime();
const quickStartProfileSubtitle = isNativeDesktopRuntime
  ? "Enter your name to get started. A standard local Nostr account is created automatically and its private key can be exported later from Settings."
  : "Enter your name to get started. This browser preview only prepares a temporary session. Launch the Tauri desktop shell to generate and export a real local Nostr private key.";

const seededName = splitName(props.profile.name);
const firstName = ref(seededName.first);
const lastName = ref(seededName.last);

let timer: number | undefined;

const selectedRestorableCircles = computed(() => {
  return props.restorableCircles.filter((circle) =>
    selectedRestoreRelays.value.some((relay) => sameRelay(relay, circle.relay)),
  );
});

const hasRestorableCircles = computed(() => {
  return props.restorableCircles.length > 0;
});

const restorePageTitle = computed(() => {
  return hasRestorableCircles.value ? "Welcome Back" : "Restore Circle Access";
});

const restorePageDescription = computed(() => {
  if (hasRestorableCircles.value) {
    return `We found ${props.restorableCircles.length} circles linked to your account. Select the ones you want to restore to this device.`;
  }

  return "No private circles are preloaded on this device yet. This restore entry stays available so archived circles can appear here when local or synced restore data becomes available.";
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

const profileValid = computed(() => {
  return displayName.value.length >= 2;
});

const circleStepReady = computed(() => {
  if (!circleMode.value) {
    return false;
  }

  if (circleMode.value === "restore") {
    return canRestoreCirclesAfterLogin.value && selectedRestorableCircles.value.length > 0;
  }

  if (circleMode.value === "existing") {
    return !!selectedCircleId.value;
  }

  return true;
});

const circleSheetValid = computed(() => {
  if (activeCircleSheet.value === "custom") {
    return relayLooksValid(customRelay.value);
  }

  return false;
});

const restoreActionLabel = computed(() => {
  return selectedRestorableCircles.value.length > 1
    ? `Restore ${selectedRestorableCircles.value.length} Circles`
    : "Restore Selected Circle";
});

const selectedPrivatePlan = computed(() => {
  return PRIVATE_PLAN_OPTIONS.find((plan) => plan.id === selectedPrivatePlanId.value) ?? PRIVATE_PLAN_OPTIONS[1];
});

const privateOverviewStartingPrice = computed(() => {
  return PRIVATE_PLAN_OPTIONS[0].monthlyPrice;
});

const privateYearlySavingsPercent = computed(() => {
  const monthly = parsePriceAmount(selectedPrivatePlan.value.monthlyPrice);
  const yearly = parsePriceAmount(selectedPrivatePlan.value.yearlyPrice);
  if (!monthly || !yearly) {
    return null;
  }

  const twelveMonths = monthly * 12;
  if (!twelveMonths) {
    return null;
  }

  return Math.round(Math.max(0, Math.min(100, ((twelveMonths - yearly) / twelveMonths) * 100)));
});

const privatePageTitle = computed(() => {
  if (activeCirclePage.value === "privateOverview") {
    return "OVERVIEW";
  }

  if (activeCirclePage.value === "privateLearnMore") {
    return "Learn More";
  }

  return "";
});

const privateProgressStep = computed(() => {
  if (activeCirclePage.value === "privateCapacity") {
    return 1;
  }

  if (activeCirclePage.value === "privateDuration") {
    return 2;
  }

  if (activeCirclePage.value === "privateCheckout") {
    return 3;
  }

  return 0;
});

const privateBillingLabel = computed(() => {
  return selectedPrivateBilling.value === "yearly" ? "Yearly" : "Monthly";
});

const privateDurationSubtitle = computed(() => {
  return privateYearlySavingsPercent.value !== null
    ? `Save up to ${privateYearlySavingsPercent.value}% with yearly billing.`
    : "Save with yearly billing.";
});

const privateDurationSaveLabel = computed(() => {
  return privateYearlySavingsPercent.value !== null
    ? `SAVE ${privateYearlySavingsPercent.value}%`
    : "Save";
});

const privateSelectedPrice = computed(() => {
  return selectedPrivateBilling.value === "yearly"
    ? selectedPrivatePlan.value.yearlyPrice
    : selectedPrivatePlan.value.monthlyPrice;
});

const privateSelectedPriceSuffix = computed(() => {
  return selectedPrivateBilling.value === "yearly" ? "/year" : "/month";
});

const privateInviteRemaining = computed(() => {
  return Math.max(selectedPrivatePlan.value.maxUsers - 1, 0);
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
  if (loginPreparationBusy.value) {
    return "Loading...";
  }

  if (currentStep.value === 1) {
    return "LOGIN";
  }

  if (currentStep.value === 2) {
    return "Next";
  }

  if (currentStep.value === 3) {
    if (circleMode.value === "invite" || circleMode.value === "privatePreview") {
      return "Continue to Circle Setup";
    }

    return "Continue";
  }

  return "Continue";
});

watch(
  [selectedMethod, () => props.restorableCircles.length],
  ([method, restorableCount]) => {
    if (circleMode.value === "restore" && (method === "quickStart" || restorableCount === 0)) {
      circleMode.value = defaultCircleModeForMethod(method);
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
          selectedMethod.value !== "quickStart" && props.restorableCircles.length ? "restore" : defaultCircleModeForMethod(selectedMethod.value);
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
  [selectedMethod, normalizedAccountKey, displayName, normalizedHandle, profileStatus],
  () => {
    loginPreparationError.value = "";
  },
);

watch(
  () => props.restorableCircles,
  (restorableCircles) => {
    if (!restorableCircles.length) {
      selectedRestoreRelays.value = [];
      if (activeCirclePage.value === "restore") {
        activeCirclePage.value = null;
      }
      if (circleMode.value === "restore") {
        circleMode.value = defaultCircleModeForMethod(selectedMethod.value);
      }
      return;
    }

    const availableRelays = restorableCircles.map((circle) => circle.relay);
    const nextSelectedRelays = selectedRestoreRelays.value.filter((relay) =>
      availableRelays.some((candidate) => sameRelay(candidate, relay)),
    );
    selectedRestoreRelays.value = nextSelectedRelays.length ? nextSelectedRelays : availableRelays;
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

  revokeAvatarPreviewUrl();
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

function splitName(value: string) {
  const tokens = value.trim().split(/\s+/).filter(Boolean);
  return {
    first: tokens[0] ?? "",
    last: tokens.slice(1).join(" "),
  };
}

function resolvePublicRelayShortcut(value: string) {
  const normalized = value.trim().toLowerCase();
  if (!normalized) {
    return null;
  }

  return PUBLIC_RELAY_SHORTCUTS[normalized as keyof typeof PUBLIC_RELAY_SHORTCUTS] ?? null;
}

function normalizeRelayLikeValue(value: string) {
  const trimmed = value.trim();
  if (!trimmed) {
    return "";
  }

  return resolvePublicRelayShortcut(trimmed) ?? (trimmed.includes("://") ? trimmed : `wss://${trimmed}`);
}

function sameRelay(left: string, right: string) {
  return normalizeRelayLikeValue(left).toLowerCase() === normalizeRelayLikeValue(right).toLowerCase();
}

function parsePriceAmount(value: string) {
  const parsed = Number.parseFloat(value.replace(/[^0-9.]+/g, ""));
  return Number.isFinite(parsed) ? parsed : null;
}

function isRestoreCircleSelected(relay: string) {
  return selectedRestoreRelays.value.some((candidate) => sameRelay(candidate, relay));
}

function toggleRestoreCircle(relay: string) {
  if (isRestoreCircleSelected(relay)) {
    selectedRestoreRelays.value = selectedRestoreRelays.value.filter((candidate) => !sameRelay(candidate, relay));
    return;
  }

  selectedRestoreRelays.value = [...selectedRestoreRelays.value, relay];
}

function defaultCircleModeForMethod(_method: LoginMethod): CircleSelectionMode | null {
  return null;
}

function resetLoginFlow() {
  const nextName = splitName(props.profile.name);
  revokeAvatarPreviewUrl();
  currentSlide.value = 0;
  currentStep.value = 0;
  selectedMethod.value = "quickStart";
  accountKey.value = "";
  handle.value = props.profile.handle;
  profileStatus.value = props.profile.status;
  circleMode.value = defaultCircleModeForMethod("quickStart");
  selectedCircleId.value = props.circles[0]?.id ?? "";
  selectedRestoreRelays.value = props.restorableCircles.map((circle) => circle.relay);
  customCircleName.value = "";
  customRelay.value = "";
  activeCircleSheet.value = null;
  activeCirclePage.value = null;
  activeInfoSheet.value = null;
  privateCircleName.value = "My Private Circle";
  selectedPrivatePlanId.value = "family";
  selectedPrivateBilling.value = "yearly";
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

  return HEX_KEY_PATTERN.test(trimmed) ? "hexKey" : null;
}

function openQuickStart() {
  selectedMethod.value = "quickStart";
  currentStep.value = 2;
  circleMode.value = defaultCircleModeForMethod("quickStart");
  activeCircleSheet.value = null;
  activeCirclePage.value = null;
  activeInfoSheet.value = null;
}

function openExistingAccount() {
  selectedMethod.value = "existingAccount";
  currentStep.value = 1;
  circleMode.value = defaultCircleModeForMethod("existingAccount");
  activeCircleSheet.value = null;
  activeCirclePage.value = null;
  activeInfoSheet.value = null;
}

function goBack() {
  if (activeInfoSheet.value) {
    activeInfoSheet.value = null;
    return;
  }

  if (activeCirclePage.value === "privateLearnMore") {
    activeCirclePage.value = "privateOverview";
    return;
  }

  if (activeCirclePage.value === "privateCapacity") {
    activeCirclePage.value = "privateOverview";
    return;
  }

  if (activeCirclePage.value === "privateDuration") {
    activeCirclePage.value = "privateCapacity";
    return;
  }

  if (activeCirclePage.value === "privateCheckout") {
    activeCirclePage.value = "privateDuration";
    return;
  }

  if (activeCirclePage.value === "privateActivated") {
    activeCirclePage.value = "privateCheckout";
    return;
  }

  if (activeCirclePage.value === "restore") {
    activeCirclePage.value = null;
    circleMode.value = defaultCircleModeForMethod(selectedMethod.value);
    return;
  }

  if (activeCirclePage.value) {
    activeCirclePage.value = null;
    return;
  }

  activeCircleSheet.value = null;
  if (currentStep.value === 1 || currentStep.value === 2) {
    currentStep.value = 0;
    return;
  }

  currentStep.value = selectedMethod.value === "quickStart" ? 2 : 1;
}

async function handlePrimaryAction() {
  if (!stepValid.value) {
    return;
  }

  if (currentStep.value === 1) {
    const prepared = await prepareLoginBeforeCircleSelection();
    if (!prepared) {
      return;
    }

    if (canRestoreCirclesAfterLogin.value) {
      currentStep.value = 3;
      openRestorePage();
      return;
    }

    if (canReuseExistingCircleImmediately.value) {
      submit();
      return;
    }

    currentStep.value = 3;
    return;
  }

  if (currentStep.value === 2) {
    const prepared = await prepareLoginBeforeCircleSelection();
    if (!prepared) {
      return;
    }

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

function buildCustomCircleName() {
  const trimmed = customCircleName.value.trim();
  if (trimmed) {
    return trimmed;
  }

  const normalizedRelay = normalizeRelayLikeValue(customRelay.value);
  if (normalizedRelay) {
    return normalizedRelay;
  }

  return "Custom Relay";
}

function buildSubmittedUserProfile() {
  return {
    name: displayName.value || props.profile.name || "XChat User",
    handle: normalizedHandle.value,
    initials: buildInitials(displayName.value || props.profile.name),
    status: profileStatus.value.trim() || "Circle member",
  };
}

function buildPreparationCircleSelection(): LoginCompletionInput["circleSelection"] {
  if (selectedMethod.value === "existingAccount" && hasRestorableCircles.value) {
    return {
      mode: "restore",
      relay: props.restorableCircles[0]?.relay,
      relays: props.restorableCircles.map((circle) => circle.relay),
    };
  }

  if (selectedMethod.value === "existingAccount" && canReuseExistingCircleImmediately.value) {
    return {
      mode: "existing",
      circleId: selectedCircleId.value || props.circles[0]?.id || "",
    };
  }

  return {
    mode: "invite",
  };
}

function buildLoginPreparationInput(): LoginCompletionInput {
  const resolvedAccessKind = accountKeyAccessKind.value;
  const access: LoginAccessInput =
    selectedMethod.value === "quickStart"
      ? {
          kind: "hexKey",
        }
      : {
          kind: resolvedAccessKind ?? "hexKey",
          value: normalizedAccountKey.value,
        };

  return {
    method: deriveSubmittedMethod(access.kind),
    access,
    userProfile: buildSubmittedUserProfile(),
    circleSelection: buildPreparationCircleSelection(),
    loggedInAt: new Date().toISOString(),
  };
}

async function prepareLoginBeforeCircleSelection() {
  if (loginPreparationBusy.value) {
    return false;
  }

  loginPreparationBusy.value = true;
  loginPreparationError.value = "";

  try {
    await bootstrapAuthSession(buildLoginPreparationInput());
    return true;
  } catch (error) {
    loginPreparationError.value = describePairingError(
      error,
      selectedMethod.value === "quickStart"
        ? "Profile setup could not prepare the account runtime."
        : "Login could not prepare the account runtime.",
    );
    return false;
  } finally {
    loginPreparationBusy.value = false;
  }
}

function selectCircleMode(mode: CircleSelectionMode) {
  circleMode.value = mode;
  if (mode !== "custom") {
    activeCircleSheet.value = null;
  }
}

function openCircleSheet(mode: CircleSheetMode) {
  if (mode === "custom" && !customRelay.value.trim()) {
    customRelay.value = normalizeRelayLikeValue("damus");
  }

  circleMode.value = mode;
  activeCircleSheet.value = mode;
}

function closeCircleSheet() {
  activeCircleSheet.value = null;
}

function openRestorePage() {
  if (!hasRestorableCircles.value) {
    return;
  }

  circleMode.value = "restore";
  activeCirclePage.value = "restore";
}

function openPrivateLearnMore() {
  activeCirclePage.value = "privateLearnMore";
}

function openPrivateCapacity() {
  activeCirclePage.value = "privateCapacity";
}

function openPrivateDuration() {
  activeCirclePage.value = "privateDuration";
}

function openPrivateCheckout() {
  activeCirclePage.value = "privateCheckout";
}

function openPrivateActivated() {
  activeCirclePage.value = "privateActivated";
}

function closeCirclePage() {
  activeCirclePage.value = null;
}

function skipRestorePage() {
  activeCirclePage.value = null;
  if (selectedMethod.value === "existingAccount" && canReuseExistingCircleImmediately.value) {
    submit();
    return;
  }

  circleMode.value = defaultCircleModeForMethod(selectedMethod.value);
}

function confirmRestorePage() {
  if (!selectedRestorableCircles.value.length) {
    return;
  }

  if (!canRestoreCirclesAfterLogin.value) {
    activeCirclePage.value = null;
    selectedMethod.value = "existingAccount";
    currentStep.value = 1;
    circleMode.value = defaultCircleModeForMethod("existingAccount");
    return;
  }

  activeCirclePage.value = null;
  submit();
}

function editPrivateCircleName() {
  const nextValue = window.prompt("Name your circle", privateCircleName.value)?.trim();
  if (nextValue) {
    privateCircleName.value = nextValue;
  }
}

async function sharePrivateInvitePreview() {
  const inviteLink = "https://0xchat.com/x/invite/private-circle-preview";
  const webShare = (navigator as Navigator & {
    share?: (data: { title?: string; url?: string }) => Promise<void>;
  }).share;

  try {
    if (typeof webShare === "function") {
      await webShare({
        title: privateCircleName.value,
        url: inviteLink,
      });
      return;
    }
  } catch {
    return;
  }

  try {
    await navigator.clipboard.writeText(inviteLink);
  } catch {}
}

function openInfoSheet(kind: InfoSheetKind) {
  activeInfoSheet.value = kind;
}

function closeInfoSheet() {
  activeInfoSheet.value = null;
}

function describePairingError(error: unknown, fallback: string) {
  if (error instanceof Error && error.message.trim()) {
    return error.message.trim();
  }

  if (typeof error === "string" && error.trim()) {
    return error.trim();
  }

  return fallback;
}

async function openExternalUrl(url: string) {
  try {
    await openUrl(url);
    return;
  } catch {}

  window.open(url, "_blank", "noopener,noreferrer");
}

function openPrivacyPolicy() {
  return openExternalUrl("https://0xchat.com/protocols/xchat-privacy-policy.html");
}

function openTermsOfUse() {
  return openExternalUrl("https://0xchat.com/protocols/xchat-terms-of-use.html");
}

function revokeAvatarPreviewUrl() {
  if (avatarPreviewUrl.value.startsWith("blob:")) {
    URL.revokeObjectURL(avatarPreviewUrl.value);
  }

  avatarPreviewUrl.value = "";
}

function openAvatarPicker() {
  avatarInput.value?.click();
}

function handleAvatarSelected(event: Event) {
  const input = event.target as HTMLInputElement | null;
  const file = input?.files?.[0];
  if (!file) {
    return;
  }

  revokeAvatarPreviewUrl();
  avatarPreviewUrl.value = URL.createObjectURL(file);
  input.value = "";
}

function applyRelaySuggestion(value: string) {
  customRelay.value = normalizeRelayLikeValue(value);
}

function handleCircleConnect() {
  if (!circleMode.value) {
    return;
  }

  if (circleMode.value === "restore" || circleMode.value === "existing") {
    if (circleMode.value === "restore" && !selectedRestorableCircles.value.length) {
      openRestorePage();
      return;
    }

    submit();
    return;
  }

  if (circleMode.value === "invite" || circleMode.value === "privatePreview") {
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
  if (!stepValid.value || !circleMode.value) {
    return;
  }

  const resolvedAccessKind = accountKeyAccessKind.value;
  const access: LoginAccessInput =
    selectedMethod.value === "quickStart"
      ? {
          kind: "hexKey",
        }
      : {
          kind: resolvedAccessKind ?? "hexKey",
          value: normalizedAccountKey.value,
        };
  const method = deriveSubmittedMethod(access.kind);

  const payload: LoginCompletionInput = {
    method,
    access,
    userProfile: buildSubmittedUserProfile(),
    circleSelection:
      selectedMethod.value === "existingAccount" && canReuseExistingCircleImmediately.value
        ? {
            mode: "existing",
            circleId: selectedCircleId.value || props.circles[0]?.id || "",
          }
        : circleMode.value === "invite" || circleMode.value === "privatePreview"
            ? {
                mode: "invite",
              }
            : circleMode.value === "restore"
              ? {
                  mode: "restore",
                  relay: selectedRestorableCircles.value[0]?.relay,
                  relays: selectedRestorableCircles.value.map((circle) => circle.relay),
                }
              : {
                  mode: "custom",
                  name: buildCustomCircleName(),
                  relay: normalizeRelayLikeValue(customRelay.value),
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
          <p>
            By continuing, you agree to our
            <button type="button" class="terms-link-button" @click="openPrivacyPolicy">Privacy Policy</button>
            and
            <button type="button" class="terms-link-button" @click="openTermsOfUse">Terms of Use</button>
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
            <h2 class="page-title page-title-left">Use your Nostr private key to log in:</h2>

            <Textarea
              id="account-key"
              v-model="accountKey"
              rows="4"
              auto-resize
              class="wide-input"
              placeholder="Enter nsec or 64-character hex private key (0x optional)"
            />

            <p class="section-footnote">
              Enter your Nostr private key in `nsec` or 64-character hex format. `0x` is optional for hex keys.
              <button type="button" class="inline-link-button" @click="openInfoSheet('nostr')">Learn more</button>
            </p>

            <p v-if="invalidNsecHintVisible" class="inline-hint tone-error">
              The NSEC you entered is invalid. Please enter it again.
            </p>
          </section>
        </template>

        <template v-else-if="currentStep === 2">
          <section class="page-section profile-section">
            <div class="page-copy centered-copy">
              <h2 class="page-title">Create Your Profile</h2>
              <p class="page-subtitle">{{ quickStartProfileSubtitle }}</p>
            </div>

            <button type="button" class="avatar-preview" aria-label="Select avatar" @click="openAvatarPicker">
              <img v-if="avatarPreviewUrl" :src="avatarPreviewUrl" alt="" class="profile-avatar-image" />
              <span v-else class="profile-avatar">{{ buildInitials(displayName || "XC") }}</span>
              <span class="avatar-badge">
                <i class="pi pi-camera" />
              </span>
            </button>
            <input
              ref="avatarInput"
              type="file"
              accept="image/*"
              class="hidden-file-input"
              @change="handleAvatarSelected"
            />

            <div class="input-stack">
              <InputText id="first-name" v-model="firstName" class="wide-input" placeholder="First Name" />
              <InputText id="last-name" v-model="lastName" class="wide-input" placeholder="Last Name (Optional)" />
            </div>

            <p class="section-footnote">This is how others will see you. You can change this anytime.</p>
          </section>
        </template>

        <template v-else>
          <section class="page-section circle-section">
            <div class="page-copy centered-copy">
              <h2 class="page-title">Add Circle</h2>
              <p class="page-subtitle">Choose how to continue into circle setup after login.</p>
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
                  <small>Open the invite handoff after login</small>
                </span>
                <i class="pi pi-angle-right option-arrow" />
              </button>

              <div class="option-divider">
                <span></span>
                <p>OR CONNECT VIA</p>
                <span></span>
              </div>

              <div class="recommended-card">
                <span class="recommended-badge">Most Popular</span>
                <button
                  type="button"
                  :class="['option-card', { active: circleMode === 'privatePreview' }]"
                  @click="selectCircleMode('privatePreview')"
                >
                  <span class="option-icon">
                    <i class="pi pi-diamond" />
                  </span>
                  <span class="option-copy">
                    <strong>Get Private Circle</strong>
                    <small>Continue to the Private Circle handoff after login</small>
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
                  <small>Use a community or self-hosted relay</small>
                </span>
                <i class="pi pi-angle-right option-arrow" />
              </button>
            </div>

            <button
              type="button"
              :class="['restore-link', { active: circleMode === 'restore', disabled: !hasRestorableCircles }]"
              :disabled="!hasRestorableCircles"
              @click="openRestorePage"
            >
              Restore saved circles
            </button>
          </section>
        </template>
      </div>

      <footer class="mobile-footer">
        <p v-if="loginPreparationError && currentStep !== 3" class="inline-hint tone-error mobile-footer-hint">
          {{ loginPreparationError }}
        </p>
        <button
          type="button"
          class="action-button action-dark"
          :disabled="!stepValid || loginPreparationBusy"
          @click="handlePrimaryAction"
        >
          {{ primaryActionLabel }}
        </button>
      </footer>
    </div>

    <Transition name="sheet-fade">
      <div v-if="activeCircleSheet" class="circle-sheet-layer" @click.self="closeCircleSheet">
        <div class="circle-sheet-card" role="dialog" aria-modal="true">
          <span class="circle-sheet-grabber" aria-hidden="true"></span>

          <div class="circle-sheet-copy">
            <h3>Add Custom Relay</h3>
            <p>
              Enter the relay address to join.
              <button type="button" class="inline-link-button" @click="openInfoSheet('relay')">What is a Relay?</button>
            </p>
          </div>

          <div class="circle-sheet-body">
            <template v-if="activeCircleSheet === 'custom'">
              <label class="field-label" for="custom-relay-sheet">Enter relay URL or name</label>
              <InputText
                id="custom-relay-sheet"
                v-model="customRelay"
                class="wide-input"
                placeholder="Enter relay URL or name"
              />

              <p class="section-footnote sheet-footnote">
                You can enter a full URL (e.g., <code>wss://relay.example.com</code>) or use a shortcut like
                <button type="button" class="inline-relay-shortcut" @click="applyRelaySuggestion('0xchat')">{{ PUBLIC_RELAY_SHORTCUTS["0xchat"] }}</button>
                or
                <button type="button" class="inline-relay-shortcut" @click="applyRelaySuggestion('damus')">{{ PUBLIC_RELAY_SHORTCUTS.damus }}</button>.
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
              Join
            </button>
          </div>
        </div>
      </div>
    </Transition>

    <Transition name="sheet-fade">
      <div v-if="activeCirclePage" class="circle-page-layer">
        <div class="circle-page-shell" role="dialog" aria-modal="true">
          <header class="circle-page-header">
            <button type="button" class="topbar-back" aria-label="Go back" @click="goBack">
              <span class="topbar-chevron">‹</span>
            </button>
            <h2 class="circle-page-title">{{ privatePageTitle }}</h2>
            <span class="topbar-side" aria-hidden="true"></span>
          </header>

          <div v-if="activeCirclePage === 'restore'" class="circle-page-body restore-page">
            <div class="restore-copy">
              <div class="restore-icon">
                <i class="pi pi-cloud" />
              </div>
              <h3>{{ restorePageTitle }}</h3>
              <p>{{ restorePageDescription }}</p>
            </div>

            <div v-if="props.restorableCircles.length" class="restore-list">
              <button
                v-for="circle in props.restorableCircles"
                :key="circle.relay"
                type="button"
                :class="['restore-card', { active: isRestoreCircleSelected(circle.relay) }]"
                @click="toggleRestoreCircle(circle.relay)"
              >
                <div class="restore-avatar">
                  {{ buildInitials(circle.name) }}
                </div>
                <div class="restore-card-copy">
                  <strong>{{ circle.name }}</strong>
                  <p>{{ circle.relay }}</p>
                </div>
                <span :class="['restore-check', { active: isRestoreCircleSelected(circle.relay) }]">
                  <i v-if="isRestoreCircleSelected(circle.relay)" class="pi pi-check" />
                </span>
              </button>
            </div>
            <p v-else class="inline-hint tone-muted">No saved circles are ready to restore yet.</p>
          </div>

          <div v-else-if="activeCirclePage === 'privateOverview'" class="circle-page-body private-overview-page">
            <div class="private-hero">
              <div class="private-hero-icon">
                <i class="pi pi-diamond" />
              </div>
              <h3>Get Private Circle</h3>
              <p>Own your data with a dedicated relay and file server.</p>
              <p class="private-hero-price">Starts at just {{ privateOverviewStartingPrice }}/mo.</p>
              <button type="button" class="inline-link-button private-learn-more" @click="openPrivateLearnMore">
                Learn More
                <i class="pi pi-arrow-right" />
              </button>
            </div>

            <div class="private-feature-list">
              <article class="private-feature">
                <span class="private-feature-icon lavender"><i class="pi pi-server" /></span>
                <div>
                  <strong>Your Private Relay</strong>
                  <p>Get a dedicated Nostr relay instance exclusive to your circle.</p>
                </div>
              </article>
              <article class="private-feature">
                <span class="private-feature-icon blue"><i class="pi pi-images" /></span>
                <div>
                  <strong>Media File Server</strong>
                  <p>Includes a secure file server for your photos and videos.</p>
                </div>
              </article>
              <article class="private-feature">
                <span class="private-feature-icon rose"><i class="pi pi-shield" /></span>
                <div>
                  <strong>Total Sovereignty</strong>
                  <p>Privacy first. Permanently wipe your relay and data anytime.</p>
                </div>
              </article>
            </div>
          </div>

          <div v-else-if="activeCirclePage === 'privateLearnMore'" class="circle-page-body private-learn-page">
            <div class="learn-brand">
              <p>Get Private Circle</p>
              <span></span>
            </div>

            <div class="learn-section">
              <h4>Features</h4>
              <article class="info-card">
                <p>Dedicated hosted relay server</p>
              </article>
              <article class="info-card">
                <p>Free file storage service, 50MB per upload, files stored for 30 days</p>
              </article>
              <article class="info-card">
                <p>Member management and file cleanup</p>
              </article>
            </div>

            <div class="learn-section">
              <h4>Privacy</h4>
              <article class="info-card">
                <p>Zero logging to protect your privacy</p>
              </article>
              <article class="info-card">
                <p>Encrypted storage. Server cannot view file contents</p>
              </article>
              <article class="info-card">
                <p>When admin deletes circle, all members' local data will be automatically cleared</p>
              </article>
              <article class="info-card">
                <p>When member is removed, their local data will be automatically cleared</p>
              </article>
            </div>

            <div class="learn-section">
              <h4>Pricing</h4>
              <article class="learn-pricing-card">
                <div
                  v-for="(plan, index) in PRIVATE_PLAN_OPTIONS"
                  :key="plan.id"
                  :class="['learn-pricing-item', { divided: index > 0 }]"
                >
                  <div class="learn-pricing-head">
                    <strong>{{ plan.planName }}</strong>
                    <span>Yearly plan</span>
                  </div>
                  <div class="learn-pricing-values">
                    <strong>{{ plan.monthlyPrice }}/month</strong>
                    <span>{{ plan.yearlyPrice }}/year</span>
                  </div>
                </div>
              </article>
              <p class="learn-disclaimer">
                Subscriptions automatically renew unless auto-renew is turned off at least 24-hours before the end of
                the current period.
              </p>
            </div>
          </div>

          <div v-else-if="activeCirclePage === 'privateCapacity'" class="circle-page-body private-capacity-page">
            <div class="private-progress">
              <div class="private-progress-bars">
                <span
                  v-for="(label, index) in PRIVATE_PROGRESS_LABELS"
                  :key="label"
                  :class="['private-progress-bar', { active: index < privateProgressStep }]"
                />
              </div>
              <div class="private-progress-labels">
                <span v-for="label in PRIVATE_PROGRESS_LABELS" :key="label">{{ label }}</span>
              </div>
            </div>

            <div class="private-step-copy">
              <h3>How big is your circle?</h3>
              <p>Choose a capacity that fits your needs.</p>
            </div>

            <div class="private-option-list">
              <button
                v-for="plan in PRIVATE_PLAN_OPTIONS"
                :key="plan.id"
                type="button"
                :class="['private-select-card', { active: selectedPrivatePlanId === plan.id }]"
                @click="selectedPrivatePlanId = plan.id"
              >
                <span :class="['private-plan-icon', plan.accent]">
                  <i class="pi pi-users" />
                </span>
                <div class="private-select-copy">
                  <div class="private-select-head">
                    <strong>{{ plan.title }}</strong>
                    <span v-if="plan.mostPopular" class="private-inline-badge">Most Popular</span>
                  </div>
                  <p>{{ plan.description }}</p>
                </div>
                <span :class="['private-radio', { active: selectedPrivatePlanId === plan.id }]">
                  <i v-if="selectedPrivatePlanId === plan.id" class="pi pi-check" />
                </span>
              </button>
            </div>
          </div>

          <div v-else-if="activeCirclePage === 'privateDuration'" class="circle-page-body private-duration-page">
            <div class="private-progress">
              <div class="private-progress-bars">
                <span
                  v-for="(label, index) in PRIVATE_PROGRESS_LABELS"
                  :key="label"
                  :class="['private-progress-bar', { active: index < privateProgressStep }]"
                />
              </div>
              <div class="private-progress-labels">
                <span v-for="label in PRIVATE_PROGRESS_LABELS" :key="label">{{ label }}</span>
              </div>
            </div>

            <div class="private-step-copy">
              <h3>How often to pay?</h3>
              <p>{{ privateDurationSubtitle }}</p>
            </div>

            <div class="private-option-list">
              <button
                type="button"
                :class="['private-select-card', { active: selectedPrivateBilling === 'yearly' }]"
                @click="selectedPrivateBilling = 'yearly'"
              >
                <div class="private-select-copy">
                  <div class="private-select-head">
                    <strong>Yearly</strong>
                    <span v-if="selectedPrivateBilling === 'yearly'" class="private-save-badge">{{ privateDurationSaveLabel }}</span>
                  </div>
                  <span class="private-price-line">{{ selectedPrivatePlan.yearlyPrice }}</span>
                  <p>Billed every 12 months</p>
                </div>
                <span :class="['private-radio', { active: selectedPrivateBilling === 'yearly' }]">
                  <i v-if="selectedPrivateBilling === 'yearly'" class="pi pi-check" />
                </span>
              </button>

              <button
                type="button"
                :class="['private-select-card', { active: selectedPrivateBilling === 'monthly' }]"
                @click="selectedPrivateBilling = 'monthly'"
              >
                <div class="private-select-copy">
                  <div class="private-select-head">
                    <strong>Monthly</strong>
                  </div>
                  <span class="private-price-line">{{ selectedPrivatePlan.monthlyPrice }}</span>
                  <p>Billed every month</p>
                </div>
                <span :class="['private-radio', { active: selectedPrivateBilling === 'monthly' }]">
                  <i v-if="selectedPrivateBilling === 'monthly'" class="pi pi-check" />
                </span>
              </button>
            </div>
          </div>

          <div v-else-if="activeCirclePage === 'privateCheckout'" class="circle-page-body private-checkout-page">
            <div class="private-progress">
              <div class="private-progress-bars">
                <span
                  v-for="(label, index) in PRIVATE_PROGRESS_LABELS"
                  :key="label"
                  :class="['private-progress-bar', { active: index < privateProgressStep }]"
                />
              </div>
              <div class="private-progress-labels">
                <span v-for="label in PRIVATE_PROGRESS_LABELS" :key="label">{{ label }}</span>
              </div>
            </div>

            <div class="private-step-copy">
              <h3>Review Order</h3>
              <p>You won't be charged until you confirm.</p>
            </div>

            <article class="private-summary-card">
              <div class="private-summary-row">
                <div class="private-summary-copy">
                  <span class="private-summary-label">Plan</span>
                  <strong>{{ selectedPrivatePlan.planName }}</strong>
                  <p>{{ selectedPrivatePlan.maxUsers }} Max Users • Unlimited Secure Storage</p>
                </div>
                <div class="private-summary-side">
                  <span class="private-summary-label">Billing</span>
                  <strong>{{ privateBillingLabel }}</strong>
                </div>
              </div>

              <div class="private-summary-divider"></div>

              <div class="private-summary-total">
                <strong>Total</strong>
                <div class="private-summary-price">
                  <span>{{ privateSelectedPrice }}</span>
                  <small>{{ privateSelectedPriceSuffix }}</small>
                </div>
              </div>
            </article>

            <p class="checkout-terms">
              By subscribing you agree to our
              <button type="button" class="checkout-link" @click="openPrivacyPolicy">Privacy Policy</button>
              and
              <button type="button" class="checkout-link" @click="openTermsOfUse">Terms of Use</button>.
            </p>
          </div>

          <div v-else class="circle-page-body private-activated-page">
            <div class="activated-success">
              <div class="activated-success-icon">
                <i class="pi pi-check-circle" />
              </div>
              <h3>Circle Activated!</h3>
              <p>Your secure private relay is live. Customize your circle to get started.</p>
            </div>

            <section class="activated-section">
              <div class="activated-step-header">
                <span class="activated-step-index">1</span>
                <strong>Name your circle</strong>
              </div>

              <button type="button" class="activated-card activated-edit-card" @click="editPrivateCircleName">
                <span>{{ privateCircleName }}</span>
                <i class="pi pi-pencil" />
              </button>
            </section>

            <section class="activated-section">
              <div class="activated-step-header">
                <span class="activated-step-index">2</span>
                <strong>Invite members</strong>
              </div>

              <div class="activated-card activated-plan-card">
                <div class="activated-plan-head">
                  <div class="activated-plan-copy">
                    <span class="activated-plan-icon"><i class="pi pi-diamond" /></span>
                    <div>
                      <strong>{{ selectedPrivatePlan.planName }}</strong>
                      <p>Unlimited Secure Storage</p>
                    </div>
                  </div>

                  <div class="activated-member-copy">
                    <strong>1/{{ selectedPrivatePlan.maxUsers }}</strong>
                    <span>MEMBERS</span>
                  </div>
                </div>

                <div class="activated-progress">
                  <span :style="{ width: `${100 / selectedPrivatePlan.maxUsers}%` }"></span>
                </div>

                <button type="button" class="activated-share-button" @click="sharePrivateInvitePreview">
                  <i class="pi pi-share-alt" />
                  <span>Share Preview Link</span>
                </button>

                <p class="activated-hint">
                  Share a private-circle preview with up to {{ privateInviteRemaining }} more people.
                </p>
              </div>
            </section>
          </div>

          <footer v-if="activeCirclePage !== 'privateLearnMore'" class="circle-page-footer">
            <template v-if="activeCirclePage === 'restore'">
              <button type="button" class="action-button action-dark" :disabled="!selectedRestorableCircles.length" @click="confirmRestorePage">
                {{ restoreActionLabel }}
              </button>
              <button type="button" class="restore-skip-button" @click="skipRestorePage">Skip</button>
            </template>

            <template v-else-if="activeCirclePage === 'privateOverview'">
              <button type="button" class="action-button action-dark" @click="openPrivateCapacity">
                <span class="action-button-content">
                  <span>CONFIGURE PLAN</span>
                  <i class="pi pi-arrow-right action-button-arrow" />
                </span>
              </button>
            </template>

            <template v-else-if="activeCirclePage === 'privateCapacity'">
              <button type="button" class="action-button action-dark" @click="openPrivateDuration">
                <span class="action-button-content">
                  <span>Continue</span>
                  <i class="pi pi-arrow-right action-button-arrow" />
                </span>
              </button>
            </template>

            <template v-else-if="activeCirclePage === 'privateDuration'">
              <button type="button" class="action-button action-dark" @click="openPrivateCheckout">
                <span class="action-button-content">
                  <span>Continue</span>
                  <i class="pi pi-arrow-right action-button-arrow" />
                </span>
              </button>
            </template>

            <template v-else-if="activeCirclePage === 'privateCheckout'">
              <button type="button" class="action-button action-dark" @click="openPrivateActivated">Subscribe</button>
            </template>

            <template v-else-if="activeCirclePage === 'privateActivated'">
              <button type="button" class="action-button action-dark" @click="closeCirclePage">
                <span class="action-button-content">
                  <span>Enter My Private Circle</span>
                  <i class="pi pi-arrow-right action-button-arrow" />
                </span>
              </button>
            </template>

            <template v-else>
              <button type="button" class="action-button action-dark" @click="closeCirclePage">Close</button>
            </template>
          </footer>
        </div>
      </div>
    </Transition>

    <Transition name="sheet-fade">
      <div v-if="activeInfoSheet" class="info-page-layer">
        <div class="info-page-shell" role="dialog" aria-modal="true">
          <header class="info-page-header">
            <button type="button" class="topbar-back" aria-label="Go back" @click="closeInfoSheet">
              <span class="topbar-chevron">‹</span>
            </button>
            <h2 class="info-page-title">{{ activeInfoSheet === "nostr" ? "Understanding Nostr" : "What is a Circle?" }}</h2>
            <span class="topbar-side" aria-hidden="true"></span>
          </header>

          <div class="info-page-body">
            <template v-if="activeInfoSheet === 'nostr'">
              <section class="info-page-section info-page-hero">
                <p class="info-page-subtitle">Learn about Nostr and how to use it securely</p>
                <span class="info-page-accent"></span>
              </section>

              <section class="info-page-section">
                <h3 class="info-page-section-title">What is Nostr?</h3>
                <article class="info-content-card">
                  <span class="info-content-icon"><i class="pi pi-globe" /></span>
                  <p>
                    Nostr is a decentralized, censorship-resistant social media protocol. It allows users to
                    communicate directly without relying on centralized servers, using cryptographic keys for identity
                    and encryption.
                  </p>
                </article>
              </section>

              <section class="info-page-section">
                <h3 class="info-page-section-title">Nostr Private Key</h3>
                <article class="info-content-card">
                  <span class="info-content-icon"><i class="pi pi-key" /></span>
                  <p>
                    A Nostr private key is a secret cryptographic key that proves your identity and allows you to sign
                    messages. It's like a password that gives you access to your account.
                  </p>
                </article>
                <article class="info-content-card">
                  <span class="info-content-icon"><i class="pi pi-code" /></span>
                  <p>
                    Private keys are typically formatted as 'nsec1...' followed by a long string of characters. This
                    format makes them easy to identify and use in Nostr applications.
                  </p>
                </article>
              </section>

              <section class="info-page-section">
                <h3 class="info-page-section-title">How to Use</h3>
                <div class="info-step-card">
                  <div class="info-step-row">
                    <span class="info-content-icon"><i class="pi pi-key" /></span>
                    <p>1. Enter your `nsec` private key or 64-character hex private key (`0x` optional)</p>
                  </div>
                  <div class="info-step-row">
                    <span class="info-content-icon"><i class="pi pi-server" /></span>
                    <p>2. Choose a relay or add a custom relay</p>
                  </div>
                  <div class="info-step-row">
                    <span class="info-content-icon"><i class="pi pi-sign-in" /></span>
                    <p>3. Connect and continue into chat</p>
                  </div>
                </div>
              </section>
            </template>

            <template v-else>
              <section class="info-page-section info-page-hero">
                <p class="info-page-subtitle">Your private communication hub</p>
                <span class="info-page-accent"></span>
                <p class="info-page-description">
                  A Circle is like your own private mailroom that handles all your encrypted messages. It helps you
                  chat with friends while keeping everything secure and private.
                </p>
              </section>

              <section class="info-page-section">
                <h3 class="info-page-section-title">Think of it like a mailroom</h3>
                <div class="info-step-card">
                  <div class="info-step-row">
                    <span class="info-content-icon"><i class="pi pi-inbox" /></span>
                    <p>📬 A Circle is like a mailroom in an apartment building - it receives, stores, and delivers messages for everyone in your community.</p>
                  </div>
                  <div class="info-step-row">
                    <span class="info-content-icon"><i class="pi pi-send" /></span>
                    <p>🔒 All messages are locked in sealed envelopes - the mailroom can't read what's inside, only you and your friends can.</p>
                  </div>
                  <div class="info-step-row">
                    <span class="info-content-icon"><i class="pi pi-shield" /></span>
                    <p>🏢 Community Circles are like shared mailrooms (free and easy). Private Circles are like having your own personal mailbox (more control).</p>
                  </div>
                </div>
              </section>

              <section class="info-page-section">
                <h3 class="info-page-section-title">How does it work?</h3>
                <div class="info-step-card">
                  <div class="info-step-row">
                    <span class="info-step-badge">1</span>
                    <p>📱 You send a message to your friends through the Circle</p>
                  </div>
                  <div class="info-step-row">
                    <span class="info-step-badge">2</span>
                    <p>📦 The Circle stores it safely (like a mailbox)</p>
                  </div>
                  <div class="info-step-row">
                    <span class="info-step-badge">3</span>
                    <p>📨 Your friends pick up the message from the Circle</p>
                  </div>
                  <div class="info-step-row">
                    <span class="info-step-badge">4</span>
                    <p>✨ Your messages are encrypted and private.</p>
                  </div>
                </div>
              </section>
            </template>
          </div>
        </div>
      </div>
    </Transition>
  </section>
</template>

<style scoped>
.login-screen {
  min-height: 100vh;
  display: grid;
  place-items: center;
  padding: 12px;
  background: linear-gradient(180deg, #5e79bd 0%, #6d86c5 42%, #7c93ca 100%);
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

.action-button-content {
  width: 100%;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
}

.action-button-arrow {
  font-size: 0.95rem;
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
  display: block;
  color: rgba(246, 248, 255, 0.72);
  font-size: 0.8rem;
  line-height: 1.6;
  text-align: center;
}

.terms-row p {
  margin: 0;
}

.terms-link-button {
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  font: inherit;
  cursor: pointer;
  text-decoration: underline;
  text-underline-offset: 0.14em;
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

.footnote-divider {
  margin: 0 0.3rem;
  color: rgba(123, 140, 165, 0.9);
}

.avatar-preview {
  width: 88px;
  height: 88px;
  margin: 4px auto 0;
  padding: 0;
  border: 0;
  background: transparent;
  position: relative;
  cursor: pointer;
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

.profile-avatar-image {
  width: 80px;
  height: 80px;
  display: block;
  border-radius: 999px;
  object-fit: cover;
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

.hidden-file-input {
  display: none;
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

.mobile-footer {
  padding: 16px 30px 24px;
  background: linear-gradient(180deg, rgba(255, 255, 255, 0) 0%, #ffffff 24%);
}

.mobile-footer-hint {
  margin: 0 0 10px;
  text-align: center;
}

.circle-page-layer {
  position: fixed;
  inset: 0;
  z-index: 26;
  background: #ffffff;
}

.circle-page-shell,
.restore-copy,
.restore-list,
.restore-card,
.restore-card-copy,
.private-hero,
.private-feature-list,
.private-feature,
.learn-brand,
.learn-section,
.learn-pricing-card,
.learn-pricing-item,
.private-progress,
.private-progress-bars,
.private-progress-labels,
.private-step-copy,
.private-option-list,
.private-select-card,
.private-select-copy,
.private-summary-card,
.private-summary-copy,
.private-summary-side,
.activated-success,
.activated-section,
.activated-card {
  display: grid;
}

.circle-page-shell {
  width: min(430px, 100vw);
  min-height: 100vh;
  margin: 0 auto;
  background: #ffffff;
  grid-template-rows: auto minmax(0, 1fr) auto;
}

.circle-page-header {
  display: grid;
  grid-template-columns: 44px minmax(0, 1fr) 44px;
  align-items: center;
  padding: 16px 18px 10px;
}

.circle-page-title {
  min-height: 24px;
  margin: 0;
  text-align: center;
  font-size: 0.95rem;
  font-weight: 700;
  letter-spacing: 0.12em;
  color: #223553;
}

.circle-page-body {
  overflow: auto;
  padding: 20px 30px 24px;
}

.circle-page-footer {
  display: grid;
  gap: 12px;
  padding: 16px 30px 24px;
  background: linear-gradient(180deg, rgba(255, 255, 255, 0) 0%, #ffffff 24%);
}

.circle-page-note {
  text-align: center;
}

.restore-page,
.private-overview-page,
.private-learn-page,
.private-capacity-page,
.private-duration-page,
.private-checkout-page,
.private-activated-page {
  gap: 24px;
}

.restore-copy {
  gap: 12px;
}

.restore-copy h3,
.restore-copy p,
.restore-card-copy p,
.restore-card-copy span,
.private-hero h3,
.private-hero p,
.private-feature p,
.restore-card-copy strong,
.private-feature strong {
  margin: 0;
}

.restore-copy h3,
.private-hero h3 {
  color: #20324f;
  font-size: 1.9rem;
  line-height: 1.08;
  letter-spacing: -0.04em;
  font-weight: 700;
}

.restore-copy p,
.private-hero p,
.private-feature p,
.private-step-copy p,
.private-summary-copy p,
.activated-success p,
.activated-plan-copy p,
.activated-hint,
.learn-disclaimer,
.checkout-terms {
  color: #73859d;
  line-height: 1.6;
}

.restore-icon,
.private-hero-icon {
  width: 48px;
  height: 48px;
  display: grid;
  place-items: center;
  border-radius: 14px;
  background: #e6eefc;
  color: #2b6fce;
  font-size: 1.25rem;
}

.restore-list {
  gap: 12px;
}

.restore-card {
  width: 100%;
  grid-template-columns: auto minmax(0, 1fr) auto;
  gap: 12px;
  align-items: center;
  padding: 16px;
  border: 1px solid rgba(211, 221, 232, 0.95);
  border-radius: 16px;
  background: #ffffff;
  text-align: left;
  cursor: pointer;
}

.restore-card.active {
  border-color: rgba(83, 132, 193, 0.82);
  background: linear-gradient(180deg, #f8fbff 0%, #f6fbf8 100%);
}

.restore-avatar {
  width: 48px;
  height: 48px;
  display: grid;
  place-items: center;
  border-radius: 999px;
  background: linear-gradient(135deg, #4f8fe6 0%, #68be9a 100%);
  color: #ffffff;
  font-size: 0.92rem;
  font-weight: 700;
}

.restore-card-copy {
  gap: 4px;
  min-width: 0;
}

.restore-card-copy p,
.restore-card-copy span {
  color: #6d8098;
  font-size: 0.88rem;
  overflow: hidden;
  text-overflow: ellipsis;
}

.restore-card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.restore-check {
  width: 24px;
  height: 24px;
  display: grid;
  place-items: center;
  border-radius: 999px;
  border: 2px solid rgba(141, 159, 184, 0.38);
  color: transparent;
}

.restore-check.active {
  border-color: #2b6fce;
  background: #2b6fce;
  color: #ffffff;
}

.restore-skip-button {
  justify-self: center;
  padding: 0;
  border: 0;
  background: transparent;
  color: #61738d;
  font: inherit;
  cursor: pointer;
}

.private-hero {
  justify-items: center;
  gap: 12px;
  text-align: center;
}

.private-hero-icon {
  width: 80px;
  height: 80px;
  border-radius: 18px;
  background: #214481;
  color: #ffffff;
  font-size: 2rem;
}

.private-hero-price {
  margin: -6px 0 0;
  color: #61738d;
  font-size: 0.96rem;
  font-weight: 600;
}

.private-learn-more {
  justify-self: center;
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.private-feature-list {
  gap: 24px;
}

.private-feature {
  grid-template-columns: auto minmax(0, 1fr);
  gap: 16px;
  align-items: start;
}

.private-feature strong,
.learn-section h4 {
  color: #20324f;
  font-size: 1rem;
  font-weight: 700;
}

.private-feature-icon {
  width: 48px;
  height: 48px;
  display: grid;
  place-items: center;
  border-radius: 12px;
  font-size: 1.15rem;
}

.private-feature-icon.lavender {
  background: #ece7ff;
  color: #6f5ad9;
}

.private-feature-icon.blue {
  background: #e3f0ff;
  color: #2b80da;
}

.private-feature-icon.rose {
  background: #ffe8ef;
  color: #d94b67;
}

.learn-brand {
  gap: 8px;
}

.learn-pricing-card {
  gap: 0;
  padding: 6px 0;
  border-radius: 12px;
  border: 1px solid rgba(210, 221, 233, 0.82);
}

.learn-pricing-item {
  gap: 10px;
  padding: 12px 16px;
}

.learn-pricing-item.divided {
  border-top: 1px solid rgba(210, 221, 233, 0.82);
}

.learn-pricing-head,
.learn-pricing-values {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
}

.learn-pricing-head strong,
.learn-pricing-values strong,
.learn-pricing-values span,
.learn-pricing-head span {
  margin: 0;
}

.learn-pricing-head strong,
.learn-pricing-values strong {
  color: #20324f;
  font-weight: 700;
}

.learn-pricing-head span,
.learn-pricing-values span {
  color: #7b8ca5;
  font-size: 0.83rem;
}

.learn-brand p,
.learn-disclaimer,
.checkout-terms {
  margin: 0;
}

.learn-brand p {
  color: #2d74dd;
  font-size: 1.15rem;
  font-weight: 600;
}

.learn-brand span {
  width: 60px;
  height: 3px;
  border-radius: 999px;
  background: linear-gradient(90deg, #2d74dd 0%, rgba(45, 116, 221, 0.2) 100%);
}

.private-learn-page,
.learn-section {
  gap: 12px;
}

.pricing-info-head,
.private-select-head,
.private-summary-row,
.private-summary-total,
.activated-step-header,
.activated-plan-head,
.activated-share-button {
  display: flex;
  align-items: center;
}

.pricing-info-head {
  justify-content: space-between;
  gap: 12px;
}

.pricing-info-head strong,
.pricing-info-head span,
.private-step-copy h3,
.private-step-copy p,
.private-select-copy strong,
.private-select-copy p,
.private-summary-copy strong,
.private-summary-side strong,
.private-summary-label,
.private-summary-price span,
.private-summary-price small,
.activated-success h3,
.activated-step-header strong,
.activated-card span,
.activated-plan-copy strong,
.activated-member-copy strong,
.activated-member-copy span {
  margin: 0;
}

.pricing-info-head strong,
.pricing-info-head span {
  color: #20324f;
  font-size: 0.98rem;
  font-weight: 700;
}

.learn-disclaimer {
  font-size: 0.84rem;
}

.private-progress {
  gap: 8px;
}

.private-progress-bars {
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 4px;
}

.private-progress-bar {
  display: block;
  height: 2px;
  border-radius: 999px;
  background: rgba(111, 127, 150, 0.24);
}

.private-progress-bar.active {
  background: #2d74dd;
}

.private-progress-labels {
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
}

.private-progress-labels span {
  color: #8a97aa;
  font-size: 0.72rem;
  font-weight: 700;
  letter-spacing: 0.08em;
}

.private-progress-labels span:nth-child(2) {
  text-align: center;
}

.private-progress-labels span:nth-child(3) {
  text-align: right;
}

.private-step-copy {
  gap: 8px;
}

.private-step-copy h3 {
  color: #20324f;
  font-size: 1.9rem;
  line-height: 1.08;
  letter-spacing: -0.04em;
  font-weight: 700;
}

.private-option-list {
  gap: 16px;
}

.private-select-card {
  width: 100%;
  grid-template-columns: auto minmax(0, 1fr) auto;
  gap: 16px;
  align-items: center;
  padding: 20px;
  border: 1px solid rgba(210, 221, 233, 0.82);
  border-radius: 16px;
  background: #ffffff;
  text-align: left;
  cursor: pointer;
}

.private-select-card.active {
  border-color: #214481;
  box-shadow: inset 0 0 0 1px rgba(33, 68, 129, 0.2);
}

.private-plan-icon {
  width: 40px;
  height: 40px;
  display: grid;
  place-items: center;
  border-radius: 999px;
  font-size: 1.05rem;
}

.private-plan-icon.lavender {
  background: #f0e5ff;
  color: #6f5ad9;
}

.private-plan-icon.blue {
  background: #e5f0ff;
  color: #2d74dd;
}

.private-plan-icon.rose {
  background: #ffe5f1;
  color: #d94b67;
}

.private-select-copy {
  gap: 6px;
}

.private-select-head {
  gap: 8px;
  flex-wrap: wrap;
}

.private-select-copy strong {
  color: #20324f;
  font-size: 1rem;
  font-weight: 700;
}

.private-select-copy p {
  color: #73859d;
  font-size: 0.9rem;
}

.private-price-line {
  color: #20324f;
  font-size: 1.55rem;
  line-height: 1.1;
  font-weight: 700;
}

.private-inline-badge,
.private-save-badge {
  padding: 4px 7px;
  border-radius: 6px;
  color: #ffffff;
  font-size: 0.67rem;
  font-weight: 700;
  letter-spacing: 0.04em;
}

.private-inline-badge {
  background: #214481;
}

.private-save-badge {
  background: #2f9b61;
}

.private-radio {
  width: 20px;
  height: 20px;
  display: grid;
  place-items: center;
  border-radius: 999px;
  border: 2px solid rgba(141, 159, 184, 0.42);
  color: transparent;
}

.private-radio.active {
  border-color: #214481;
  background: #214481;
  color: #ffffff;
}

.private-summary-card {
  gap: 16px;
  padding: 20px;
  border-radius: 16px;
  background: #f7fafc;
}

.private-summary-row,
.private-summary-total,
.activated-plan-head {
  justify-content: space-between;
  gap: 16px;
}

.private-summary-copy,
.private-summary-side {
  gap: 4px;
}

.private-summary-side {
  justify-items: end;
  text-align: right;
}

.private-summary-label {
  color: #8794a7;
  font-size: 0.75rem;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.private-summary-copy strong,
.private-summary-side strong,
.private-summary-total strong {
  color: #20324f;
  font-size: 1rem;
  font-weight: 700;
}

.private-summary-divider {
  height: 1px;
  background: rgba(117, 133, 155, 0.16);
}

.private-summary-price {
  display: grid;
  justify-items: end;
}

.private-summary-price span {
  color: #20324f;
  font-size: 1.5rem;
  line-height: 1.1;
  font-weight: 700;
}

.private-summary-price small {
  color: #73859d;
  font-size: 0.85rem;
}

.checkout-terms {
  text-align: center;
  font-size: 0.82rem;
}

.checkout-link {
  padding: 0;
  border: 0;
  background: transparent;
  color: #2d74dd;
  font: inherit;
  cursor: pointer;
  text-decoration: underline;
  text-underline-offset: 0.14em;
}

.activated-success {
  justify-items: center;
  gap: 12px;
  text-align: center;
}

.activated-success-icon {
  width: 64px;
  height: 64px;
  display: grid;
  place-items: center;
  border-radius: 18px;
  background: #e5f6ed;
  color: #2f9b61;
  font-size: 2.1rem;
}

.activated-success h3 {
  color: #20324f;
  font-size: 1.9rem;
  line-height: 1.08;
  letter-spacing: -0.04em;
  font-weight: 700;
}

.activated-section {
  gap: 12px;
}

.activated-step-header {
  gap: 8px;
}

.activated-step-index {
  width: 24px;
  height: 24px;
  display: grid;
  place-items: center;
  border-radius: 999px;
  background: #172334;
  color: #ffffff;
  font-size: 0.8rem;
  font-weight: 700;
}

.activated-step-header strong {
  color: #20324f;
  font-size: 1rem;
  font-weight: 700;
}

.activated-card {
  gap: 14px;
  padding: 16px;
  border-radius: 14px;
  background: #f7fafc;
}

.activated-edit-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  border: 0;
  cursor: pointer;
  font: inherit;
  text-align: left;
}

.activated-edit-card span,
.activated-plan-copy strong,
.activated-member-copy strong {
  color: #20324f;
  font-size: 1rem;
  font-weight: 700;
}

.activated-edit-card i {
  color: #7e8ea4;
}

.activated-plan-copy {
  display: flex;
  align-items: center;
  gap: 10px;
}

.activated-plan-icon {
  width: 28px;
  height: 28px;
  display: grid;
  place-items: center;
  color: #f0b323;
  font-size: 1rem;
}

.activated-plan-copy p,
.activated-member-copy span {
  color: #7b8ca5;
  font-size: 0.83rem;
  line-height: 1.45;
  margin: 2px 0 0;
}

.activated-member-copy {
  text-align: right;
}

.activated-progress {
  height: 4px;
  border-radius: 999px;
  overflow: hidden;
  background: rgba(117, 133, 155, 0.18);
}

.activated-progress span {
  display: block;
  height: 100%;
  border-radius: inherit;
  background: #2d74dd;
}

.activated-share-button {
  justify-content: center;
  gap: 8px;
  padding: 12px 14px;
  border: 1.5px solid rgba(117, 133, 155, 0.28);
  border-radius: 12px;
  background: #ffffff;
  color: #172334;
  font: inherit;
  font-weight: 700;
  cursor: pointer;
}

.activated-hint,
.activated-feedback {
  text-align: center;
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
.circle-sheet-actions {
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

.inline-relay-shortcut {
  padding: 0;
  border: 0;
  background: transparent;
  color: #2b6fce;
  font: inherit;
  cursor: pointer;
}

.sheet-button {
  border: 0;
  cursor: pointer;
  font: inherit;
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

.info-page-layer {
  position: fixed;
  inset: 0;
  z-index: 27;
  background: #ffffff;
}

.info-page-shell {
  width: min(430px, 100vw);
  min-height: 100vh;
  margin: 0 auto;
  background: #ffffff;
}

.pairing-page-shell {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr) auto;
}

.info-page-header {
  display: grid;
  grid-template-columns: 44px minmax(0, 1fr) 44px;
  align-items: center;
  padding: 16px 18px 10px;
}

.info-page-title {
  margin: 0;
  color: #223553;
  text-align: center;
  font-size: 0.98rem;
  font-weight: 700;
}

.info-page-body,
.info-page-section,
.info-step-card,
.info-how-card,
.info-faq-card {
  display: grid;
}

.info-page-body {
  gap: 24px;
  padding: 20px 30px 24px;
}

.pairing-page-body {
  padding-bottom: 12px;
}

.info-page-section {
  gap: 16px;
}

.info-section-link {
  margin-left: 0.25rem;
}

.info-page-hero {
  gap: 8px;
}

.info-page-subtitle,
.info-page-description,
.info-content-card p,
.info-step-row p,
.info-qa-card p,
.info-how-card p,
.info-faq-card p {
  margin: 0;
}

.info-page-subtitle {
  color: #2d74dd;
  font-size: 1.15rem;
  font-weight: 600;
}

.info-page-description {
  color: #667a94;
  line-height: 1.6;
  font-size: 0.92rem;
}

.info-page-accent {
  width: 60px;
  height: 3px;
  border-radius: 999px;
  background: linear-gradient(90deg, #2d74dd 0%, rgba(45, 116, 221, 0.2) 100%);
}

.info-page-section-title {
  margin: 0;
  color: #20324f;
  font-size: 1.35rem;
  line-height: 1.2;
  font-weight: 700;
}

.info-content-card,
.info-step-card,
.info-qa-card,
.info-how-card,
.info-faq-card {
  gap: 12px;
  padding: 16px;
  border-radius: 12px;
  border: 1px solid rgba(210, 221, 233, 0.82);
  background: #ffffff;
}

.info-content-card,
.info-step-row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: 12px;
  align-items: start;
}

.info-content-icon {
  width: 40px;
  height: 40px;
  display: grid;
  place-items: center;
  border-radius: 8px;
  background: rgba(45, 116, 221, 0.1);
  color: #2d74dd;
  font-size: 1.05rem;
}

.info-content-card p,
.info-step-row p,
.info-qa-card p,
.info-how-card p,
.info-faq-card p {
  color: #667a94;
  line-height: 1.6;
  font-size: 0.92rem;
}

.info-step-badge {
  width: 24px;
  height: 24px;
  display: grid;
  place-items: center;
  border-radius: 999px;
  background: #2d74dd;
  color: #ffffff;
  font-size: 0.82rem;
  font-weight: 700;
}

.info-step-card {
  gap: 12px;
}

.info-qa-head {
  display: flex;
  align-items: flex-start;
  gap: 8px;
}

.info-qa-dot {
  width: 6px;
  height: 6px;
  margin-top: 8px;
  border-radius: 999px;
  background: #2d74dd;
}

.info-qa-head strong,
.info-faq-card strong {
  color: #20324f;
  font-size: 0.98rem;
  line-height: 1.4;
  font-weight: 700;
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

.pairing-uri-input {
  word-break: break-all;
}

.pairing-relay-list {
  word-break: break-word;
}

.pairing-page-footer {
  display: grid;
  gap: 10px;
  padding: 0 30px 24px;
}

.pairing-button {
  min-height: 48px;
  border: 0;
  border-radius: 999px;
  font: inherit;
  font-weight: 600;
  cursor: pointer;
  transition:
    transform 160ms ease,
    box-shadow 160ms ease,
    opacity 160ms ease;
}

.pairing-button:hover:enabled {
  transform: translateY(-1px);
}

.pairing-button:disabled {
  cursor: not-allowed;
  opacity: 0.52;
}

.pairing-button-secondary {
  background: #eef4fb;
  color: #27426d;
}

.pairing-button-primary {
  background: #0f1729;
  color: #ffffff;
}

.info-step-list {
  display: grid;
  gap: 10px;
  padding-left: 1.2rem;
}

.sheet-fade-enter-active,
.sheet-fade-leave-active {
  transition: opacity 0.18s ease;
}

.sheet-fade-enter-active .circle-sheet-card,
.sheet-fade-leave-active .circle-sheet-card,
.sheet-fade-enter-active .info-page-shell,
.sheet-fade-leave-active .info-page-shell {
  transition:
    transform 0.18s ease,
    opacity 0.18s ease;
}

.sheet-fade-enter-from,
.sheet-fade-leave-to {
  opacity: 0;
}

.sheet-fade-enter-from .circle-sheet-card,
.sheet-fade-leave-to .circle-sheet-card,
.sheet-fade-enter-from .info-page-shell,
.sheet-fade-leave-to .info-page-shell {
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
  .mobile-footer,
  .info-page-body,
  .pairing-page-footer {
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
