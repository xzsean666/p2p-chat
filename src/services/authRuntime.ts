import type {
  AuthRuntimeBindingSummary,
  AuthRuntimeState,
  AuthRuntimeSummary,
  AuthSessionSummary,
  LoginAccessInput,
  LoginCompletionInput,
  UpdateAuthRuntimeInput,
} from "../types/chat";

export function deriveAuthRuntimeFromAuthSession(
  authSession: AuthSessionSummary | null | undefined,
): AuthRuntimeSummary | null {
  if (!authSession) {
    return null;
  }

  if (authSession.loginMethod === "quickStart") {
    if (authSession.access.kind === "localProfile") {
      return {
        state: "localProfile",
        loginMethod: authSession.loginMethod,
        accessKind: authSession.access.kind,
        label: authSession.access.label,
        pubkey: authSession.access.pubkey,
        canSendMessages: true,
        persistedInNativeStore: false,
        credentialPersistedInNativeStore: false,
        updatedAt: authSession.loggedInAt,
      };
    }

    if (authSession.access.kind === "nsec" || authSession.access.kind === "hexKey") {
      return {
        state: "connected",
        loginMethod: authSession.loginMethod,
        accessKind: authSession.access.kind,
        label: authSession.access.label,
        pubkey: authSession.access.pubkey,
        canSendMessages: true,
        persistedInNativeStore: false,
        credentialPersistedInNativeStore: false,
        updatedAt: authSession.loggedInAt,
      };
    }
  }

  if (authSession.loginMethod === "existingAccount") {
    if (authSession.access.kind === "nsec" || authSession.access.kind === "hexKey") {
      return {
        state: "connected",
        loginMethod: authSession.loginMethod,
        accessKind: authSession.access.kind,
        label: authSession.access.label,
        pubkey: authSession.access.pubkey,
        canSendMessages: true,
        persistedInNativeStore: false,
        credentialPersistedInNativeStore: false,
        updatedAt: authSession.loggedInAt,
      };
    }

    if (authSession.access.kind === "npub") {
      return {
        state: "failed",
        loginMethod: authSession.loginMethod,
        accessKind: authSession.access.kind,
        label: authSession.access.label,
        pubkey: authSession.access.pubkey,
        error: "Read-only npub import cannot sign messages yet.",
        canSendMessages: false,
        sendBlockedReason: "Read-only npub import cannot sign messages yet.",
        persistedInNativeStore: false,
        credentialPersistedInNativeStore: false,
        updatedAt: authSession.loggedInAt,
      };
    }

    if (authSession.access.kind === "bunker" || authSession.access.kind === "nostrConnect") {
      return {
        state: "pending",
        loginMethod: authSession.loginMethod,
        accessKind: authSession.access.kind,
        label: authSession.access.label,
        pubkey: authSession.access.pubkey,
        error: "Remote signer handoff is stored and waiting for a signer handshake.",
        canSendMessages: false,
        sendBlockedReason: "Remote signer handoff is stored and waiting for a signer handshake.",
        persistedInNativeStore: false,
        credentialPersistedInNativeStore: false,
        updatedAt: authSession.loggedInAt,
      };
    }

    return {
      state: "failed",
      loginMethod: authSession.loginMethod,
      accessKind: authSession.access.kind,
      label: authSession.access.label,
      pubkey: authSession.access.pubkey,
      error: "Unsupported existing-account auth runtime input.",
      canSendMessages: false,
      sendBlockedReason: "Unsupported existing-account auth runtime input.",
      persistedInNativeStore: false,
      credentialPersistedInNativeStore: false,
      updatedAt: authSession.loggedInAt,
    };
  }

  if (authSession.access.kind === "bunker" || authSession.access.kind === "nostrConnect") {
    return {
      state: "pending",
      loginMethod: authSession.loginMethod,
      accessKind: authSession.access.kind,
      label: authSession.access.label,
      pubkey: authSession.access.pubkey,
      error: "Remote signer handshake is pending.",
      canSendMessages: false,
      sendBlockedReason: "Remote signer handshake is pending.",
      persistedInNativeStore: false,
      credentialPersistedInNativeStore: false,
      updatedAt: authSession.loggedInAt,
    };
  }

  return {
    state: "failed",
    loginMethod: authSession.loginMethod,
    accessKind: authSession.access.kind,
    label: authSession.access.label,
    pubkey: authSession.access.pubkey,
    error: "Unsupported signer auth runtime input.",
    canSendMessages: false,
    sendBlockedReason: "Unsupported signer auth runtime input.",
    persistedInNativeStore: false,
    credentialPersistedInNativeStore: false,
    updatedAt: authSession.loggedInAt,
  };
}

export function buildUpdatedAuthRuntime(
  authSession: AuthSessionSummary | null | undefined,
  currentRuntime: AuthRuntimeSummary | null | undefined,
  input: UpdateAuthRuntimeInput,
): AuthRuntimeSummary | null {
  if (!authSession) {
    return null;
  }

  if (authSession.access.kind === "localProfile" && input.state !== "localProfile") {
    return null;
  }

  if (authSession.access.kind !== "localProfile" && input.state === "localProfile") {
    return null;
  }

  const derivedRuntime = deriveAuthRuntimeFromAuthSession(authSession);
  const baseRuntime = currentRuntime ?? derivedRuntime;
  if (!baseRuntime) {
    return null;
  }

  const nextError = resolveUpdatedAuthRuntimeError(authSession, baseRuntime, input);
  const sendBlockedReason = resolveSendBlockedReason(
    authSession.access.kind,
    input.state,
    nextError,
  );

  return {
    state: input.state,
    loginMethod: authSession.loginMethod,
    accessKind: authSession.access.kind,
    label: input.label?.trim() || baseRuntime.label,
    pubkey: baseRuntime.pubkey ?? authSession.access.pubkey,
    error: nextError,
    canSendMessages: !sendBlockedReason,
    sendBlockedReason,
    persistedInNativeStore: false,
    credentialPersistedInNativeStore: baseRuntime.credentialPersistedInNativeStore,
    updatedAt: input.updatedAt?.trim() || new Date().toISOString(),
  };
}

function resolveUpdatedAuthRuntimeError(
  authSession: AuthSessionSummary,
  currentRuntime: AuthRuntimeSummary,
  input: UpdateAuthRuntimeInput,
) {
  if (input.state === "connected" || input.state === "localProfile") {
    return undefined;
  }

  const nextError = input.error?.trim();
  if (nextError) {
    return nextError;
  }

  if (currentRuntime.state === input.state && currentRuntime.error?.trim()) {
    return currentRuntime.error.trim();
  }

  return defaultAuthRuntimeError(authSession, input.state);
}

function defaultAuthRuntimeError(
  authSession: AuthSessionSummary,
  state: Extract<AuthRuntimeState, "pending" | "failed">,
) {
  if (state === "pending") {
    if (authSession.access.kind === "bunker" || authSession.access.kind === "nostrConnect") {
      return "Remote signer handshake is pending.";
    }

    return "Account runtime is still waiting for a signer handshake.";
  }

  if (authSession.access.kind === "npub") {
    return "Read-only npub import cannot sign messages yet.";
  }

  if (authSession.access.kind === "bunker" || authSession.access.kind === "nostrConnect") {
    return "Remote signer handshake failed or has not completed yet.";
  }

  return "This account runtime cannot send messages yet.";
}

function resolveSendBlockedReason(
  _accessKind: AuthSessionSummary["access"]["kind"],
  state: AuthRuntimeState,
  error: string | undefined,
) {
  if (state === "localProfile") {
    return undefined;
  }

  if (state === "connected") {
    return undefined;
  }

  if (error?.trim()) {
    return error.trim();
  }

  return state === "pending"
    ? "Account runtime is still waiting for a signer handshake."
    : "This account runtime cannot send messages yet.";
}

export function resolveAuthRuntimeSendBlockedReason(
  accessKind: AuthSessionSummary["access"]["kind"],
  state: AuthRuntimeState,
  error: string | undefined,
) {
  return resolveSendBlockedReason(accessKind, state, error);
}

export function resolveAuthRuntimeCanSendMessages(
  accessKind: AuthSessionSummary["access"]["kind"],
  state: AuthRuntimeState,
  error: string | undefined,
) {
  return !resolveSendBlockedReason(accessKind, state, error);
}

export function buildAuthRuntimeBindingSummary(
  input: Pick<LoginCompletionInput, "access" | "loggedInAt">,
  persistedInNativeStore: boolean,
): AuthRuntimeBindingSummary | null {
  if (!supportsAuthRuntimeBinding(input.access)) {
    return null;
  }

  const value = input.access.value?.trim();
  if (!value) {
    return null;
  }

  const parsedBinding = parseAuthRuntimeBinding(input.access);

  return {
    accessKind: input.access.kind,
    endpoint: parsedBinding?.endpoint || deriveAuthRuntimeBindingEndpoint(value),
    connectionPubkey:
      parsedBinding?.connectionPubkey || deriveAuthRuntimeBindingPubkey(value) || undefined,
    relayCount: parsedBinding?.relayCount ?? deriveAuthRuntimeBindingRelayCount(value),
    hasSecret: parsedBinding?.hasSecret ?? !!extractQueryParam(value, "secret"),
    requestedPermissions:
      parsedBinding?.requestedPermissions || deriveAuthRuntimeBindingPermissions(value),
    clientName: parsedBinding?.clientName || deriveAuthRuntimeBindingClientName(value) || undefined,
    persistedInNativeStore,
    updatedAt: input.loggedInAt?.trim() || new Date().toISOString(),
  };
}

type ParsedAuthRuntimeBinding = {
  endpoint: string;
  connectionPubkey: string;
  relayCount: number;
  hasSecret: boolean;
  requestedPermissions: string[];
  clientName?: string;
};

function supportsAuthRuntimeBinding(access: LoginAccessInput) {
  return access.kind === "bunker" || access.kind === "nostrConnect";
}

function parseAuthRuntimeBinding(access: LoginAccessInput): ParsedAuthRuntimeBinding | null {
  const value = access.value?.trim();
  if (!value) {
    return null;
  }

  try {
    const uri = new URL(value);
    const expectedProtocol = access.kind === "bunker" ? "bunker:" : "nostrconnect:";
    if (uri.protocol.toLowerCase() !== expectedProtocol) {
      return null;
    }

    const connectionPubkey = normalizeHexXOnlyPublicKey(uri.hostname);
    if (!connectionPubkey) {
      return null;
    }

    const relays = uri.searchParams
      .getAll("relay")
      .map((relay) => normalizeRelayUrl(relay))
      .filter((relay): relay is string => !!relay);
    if (!relays.length) {
      return null;
    }

    const hasSecret = !!uri.searchParams.get("secret")?.trim();
    if (access.kind === "nostrConnect" && !hasSecret) {
      return null;
    }

    const requestedPermissions = (uri.searchParams.get("perms") || "")
      .split(",")
      .map((permission) => permission.trim())
      .filter(Boolean);
    const clientName = uri.searchParams.get("name")?.trim() || undefined;

    return {
      endpoint: relays[0],
      connectionPubkey,
      relayCount: relays.length,
      hasSecret,
      requestedPermissions,
      clientName,
    };
  } catch {
    return null;
  }
}

function deriveAuthRuntimeBindingEndpoint(value: string) {
  return (
    extractQueryParam(value, "relay") ||
    extractQueryParam(value, "url") ||
    truncateAuthRuntimeBindingEndpoint(stripUriEndpoint(value))
  );
}

function deriveAuthRuntimeBindingPubkey(value: string) {
  const remainder = stripUriEndpoint(value);
  return normalizeHexXOnlyPublicKey(remainder);
}

function deriveAuthRuntimeBindingRelayCount(value: string) {
  const queryStart = value.indexOf("?");
  if (queryStart < 0) {
    return 0;
  }

  return value
    .slice(queryStart + 1)
    .split("&")
    .map((pair) => pair.split("=")[0]?.trim().toLowerCase())
    .filter((name) => name === "relay").length;
}

function deriveAuthRuntimeBindingPermissions(value: string) {
  const perms = extractQueryParam(value, "perms");
  if (!perms) {
    return [];
  }

  return perms
    .split(",")
    .map((permission) => permission.trim())
    .filter(Boolean);
}

function deriveAuthRuntimeBindingClientName(value: string) {
  return extractQueryParam(value, "name");
}

function extractQueryParam(value: string, key: string) {
  const queryStart = value.indexOf("?");
  if (queryStart < 0) {
    return "";
  }

  const query = value.slice(queryStart + 1);
  for (const pair of query.split("&")) {
    const [name, rawValue] = pair.split("=");
    if (name?.toLowerCase() === key.toLowerCase() && rawValue?.trim()) {
      return decodeQueryValue(rawValue.trim());
    }
  }

  return "";
}

function stripUriEndpoint(value: string) {
  const remainder = value.includes("://") ? value.split("://")[1] : value;
  const withoutQuery = remainder.split(/[?#]/)[0]?.trim() || remainder.trim();
  return withoutQuery.split("/")[0]?.trim() || withoutQuery;
}

function truncateAuthRuntimeBindingEndpoint(value: string) {
  if (value.length <= 48) {
    return value;
  }

  return `${value.slice(0, 48)}...`;
}

function normalizeHexXOnlyPublicKey(value: string) {
  const trimmed = value.trim();
  return /^[a-f0-9]{64}$/i.test(trimmed) ? trimmed.toLowerCase() : "";
}

function normalizeRelayUrl(value: string) {
  const trimmed = value.trim();
  if (!trimmed) {
    return "";
  }

  try {
    const relay = new URL(trimmed);
    if (relay.protocol !== "ws:" && relay.protocol !== "wss:") {
      return "";
    }

    return trimmed;
  } catch {
    return "";
  }
}

function decodeQueryValue(value: string) {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}
