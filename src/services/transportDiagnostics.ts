import { invoke } from "@tauri-apps/api/core";
import type {
  AdvancedPreferences,
  CircleItem,
  CircleTransportDiagnostic,
  ContactItem,
  DiscoveredPeer,
  GroupProfile,
  MessageItem,
  SessionSyncItem,
  SessionItem,
  TransportActivityItem,
  TransportCircleAction,
  TransportCircleActionInput,
  TransportEngineKind,
  TransportHealth,
  TransportMutationResult,
  TransportRuntimeSession,
  TransportSnapshot,
  TransportSnapshotInput,
} from "../types/chat";

export type TransportSnapshotLoadResult = {
  snapshot: TransportSnapshot;
  source: "tauri" | "fallback";
};

function cloneState<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function hasTauriRuntime() {
  const globalWindow = globalThis as typeof globalThis & {
    __TAURI__?: unknown;
    __TAURI_INTERNALS__?: unknown;
  };

  return typeof window !== "undefined" && ("__TAURI_INTERNALS__" in globalWindow || "__TAURI__" in globalWindow);
}

function protocolFromRelay(relay: string): CircleTransportDiagnostic["protocol"] {
  if (relay.startsWith("mesh://")) {
    return "mesh";
  }

  if (relay.startsWith("invite://")) {
    return "invite";
  }

  return "websocket";
}

function healthFromStatus(status: CircleItem["status"]): CircleTransportDiagnostic["health"] {
  if (status === "open") {
    return "online";
  }

  if (status === "connecting") {
    return "degraded";
  }

  return "offline";
}

function parseLatencyMs(value: string) {
  const digits = value.replace(/[^0-9]/g, "");
  if (!digits) {
    return undefined;
  }

  const numeric = Number.parseInt(digits, 10);
  return Number.isFinite(numeric) ? numeric : undefined;
}

function labelWithNativeSuffix(value: string) {
  return value.includes("native") ? value : `${value} · native`;
}

function previewLastSync(
  protocol: CircleTransportDiagnostic["protocol"],
  health: CircleTransportDiagnostic["health"],
) {
  if (health === "offline") {
    return "offline";
  }

  if (health === "degraded") {
    return protocol === "mesh"
      ? "native mesh runtime warmup"
      : protocol === "invite"
        ? "native invite warmup"
        : "native relay warmup";
  }

  return protocol === "mesh"
    ? "native mesh runtime active"
    : protocol === "invite"
      ? "native invite preview active"
      : "native relay runtime active";
}

function transportEngineFromAdvanced(advanced: AdvancedPreferences): TransportEngineKind {
  return advanced.experimentalTransport ? "nativePreview" : "mock";
}

function protocolLabel(protocol: CircleTransportDiagnostic["protocol"]) {
  return protocol === "mesh"
    ? "mesh relay"
    : protocol === "invite"
      ? "invite relay"
      : "websocket relay";
}

function activityLevelFromHealth(health: TransportHealth): TransportActivityItem["level"] {
  return health === "online" ? "success" : health === "degraded" ? "info" : "warn";
}

function buildRuntimeActivity(
  engine: TransportEngineKind,
  diagnostic: CircleTransportDiagnostic,
): TransportActivityItem {
  const title =
    engine === "nativePreview"
      ? diagnostic.health === "online"
        ? "Native runtime active"
        : diagnostic.health === "degraded"
          ? "Native runtime warmup"
          : "Native runtime offline"
      : diagnostic.health === "online"
        ? "Relay online"
        : diagnostic.health === "degraded"
          ? "Relay warming up"
          : "Relay offline";

  return {
    id: `runtime-${diagnostic.circleId}`,
    circleId: diagnostic.circleId,
    kind: "runtime",
    level: activityLevelFromHealth(diagnostic.health),
    title,
    detail: `${protocolLabel(diagnostic.protocol)} via ${
      engine === "nativePreview" ? "native preview engine" : "mock engine"
    } · ${diagnostic.peerCount} peers · ${diagnostic.queuedMessages} queued`,
    time: diagnostic.lastSync,
  };
}

function buildActionActivity(
  engine: TransportEngineKind,
  diagnostic: CircleTransportDiagnostic,
  action: TransportCircleAction,
): TransportActivityItem {
  const title =
    action === "connect"
      ? engine === "nativePreview"
        ? "Native runtime booted"
        : "Relay handshake started"
      : action === "disconnect"
        ? engine === "nativePreview"
          ? "Native runtime stopped"
          : "Relay disconnected"
        : action === "discoverPeers"
          ? engine === "nativePreview"
            ? "Native peer sweep finished"
            : "Peer discovery sweep finished"
          : action === "sync"
            ? engine === "nativePreview"
              ? "Native relay checkpoint saved"
              : "Relay sync finished"
            : engine === "nativePreview"
              ? "Native session merge committed"
              : "Session merge committed";
  const level =
    action === "disconnect"
      ? "warn"
      : action === "syncSessions"
        ? "success"
        : activityLevelFromHealth(diagnostic.health);

  return {
    id: `${diagnostic.circleId}-${action}-${Date.now()}`,
    circleId: diagnostic.circleId,
    kind: action,
    level,
    title,
    detail: `${protocolLabel(diagnostic.protocol)} · ${diagnostic.peerCount} peers · ${
      diagnostic.queuedMessages
    } queued · ${diagnostic.lastSync}`,
    time: "now",
  };
}

function trimTransportActivities(activities: TransportActivityItem[]) {
  const counts = new Map<string, number>();

  return activities.filter((item) => {
    const count = counts.get(item.circleId) ?? 0;
    if (count >= 6) {
      return false;
    }

    counts.set(item.circleId, count + 1);
    return true;
  });
}

function runtimeSessionActivityChanged(
  previous: TransportRuntimeSession | undefined,
  session: TransportRuntimeSession,
) {
  if (!previous) {
    return session.lastEventAt === "now" || !!session.lastFailureReason;
  }

  return (
    previous.lastEvent !== session.lastEvent ||
    previous.lastEventAt !== session.lastEventAt ||
    previous.lastFailureReason !== session.lastFailureReason ||
    previous.lastFailureAt !== session.lastFailureAt ||
    previous.lastLaunchResult !== session.lastLaunchResult ||
    previous.lastLaunchAt !== session.lastLaunchAt
  );
}

function runtimeSessionActivityLevel(session: TransportRuntimeSession): TransportActivityItem["level"] {
  if (
    session.lastFailureReason ||
    session.lastLaunchResult === "failed" ||
    session.lastEvent.includes("failed") ||
    session.lastEvent.includes("exited") ||
    session.lastEvent.includes("released")
  ) {
    return "warn";
  }

  if (
    session.lastLaunchResult === "reused" ||
    session.lastEvent.includes("reused") ||
    session.lastEvent.includes("booting")
  ) {
    return "info";
  }

  return "success";
}

function runtimeSessionActivityTitle(session: TransportRuntimeSession) {
  if (!session.lastEvent) {
    return "Runtime activity";
  }

  return session.lastEvent[0].toUpperCase() + session.lastEvent.slice(1);
}

function runtimeSessionActivityDetail(session: TransportRuntimeSession) {
  const parts = [`${session.adapterKind} adapter`, session.endpoint];

  if (session.resolvedLaunchCommand) {
    parts.push(`resolved ${session.resolvedLaunchCommand}`);
  }

  if (session.lastLaunchResult) {
    parts.push(
      session.lastLaunchPid
        ? `launch ${session.lastLaunchResult} · pid ${session.lastLaunchPid}`
        : `launch ${session.lastLaunchResult}`,
    );
  }

  if (session.lastFailureReason) {
    parts.push(session.lastFailureReason);
  } else if (session.launchError) {
    parts.push(session.launchError);
  }

  if (session.queueState !== "idle") {
    parts.push(
      `${session.queueState} queue${
        session.nextRetryIn ? ` · next ${session.nextRetryIn}` : ""
      }`,
    );
  }

  return parts.join(" · ");
}

function runtimeSessionActivityId(
  previous: TransportRuntimeSession | undefined,
  session: TransportRuntimeSession,
) {
  const lastEventToken = session.lastEvent.replace(/\s+/g, "-");
  const previousEventToken = previous?.lastEvent.replace(/\s+/g, "-") ?? "";
  const lastEventAtToken = session.lastEventAt.replace(/\s+/g, "-");

  return `runtime-event-${session.circleId}-${session.generation}-${lastEventToken}-${previousEventToken}-${lastEventAtToken}`;
}

function mergeRuntimeSessionActivities(
  activities: TransportActivityItem[],
  runtimeSessions: TransportRuntimeSession[],
  previousRuntimeSessions: TransportRuntimeSession[],
) {
  const previousIndex = new Map(previousRuntimeSessions.map((session) => [session.circleId, session]));
  const runtimeActivities = runtimeSessions
    .filter((session) => runtimeSessionActivityChanged(previousIndex.get(session.circleId), session))
    .map((session) => {
      const previous = previousIndex.get(session.circleId);
      return {
        id: runtimeSessionActivityId(previous, session),
        circleId: session.circleId,
        kind: "runtime" satisfies TransportActivityItem["kind"],
        level: runtimeSessionActivityLevel(session),
        title: runtimeSessionActivityTitle(session),
        detail: runtimeSessionActivityDetail(session),
        time: "now",
      } satisfies TransportActivityItem;
    });

  if (!runtimeActivities.length) {
    return activities;
  }

  return trimTransportActivities([...runtimeActivities, ...activities]);
}

function runtimeStateFromCircleStatus(status: CircleItem["status"]): TransportRuntimeSession["state"] {
  if (status === "open") {
    return "active";
  }

  if (status === "connecting") {
    return "starting";
  }

  return "inactive";
}

function runtimeIsLive(state: TransportRuntimeSession["state"]) {
  return state === "starting" || state === "active";
}

function runtimeDriver(
  engine: TransportEngineKind,
  protocol: CircleTransportDiagnostic["protocol"],
  useTorNetwork: boolean,
) {
  if (engine === "nativePreview") {
    if (protocol === "mesh") {
      return "native-preview-mesh-runtime";
    }

    if (protocol === "invite") {
      return "native-preview-invite-runtime";
    }

    return useTorNetwork ? "native-preview-tor-runtime" : "native-preview-relay-runtime";
  }

  if (protocol === "mesh") {
    return "local-mock-mesh-daemon";
  }

  if (protocol === "invite") {
    return "local-mock-invite-daemon";
  }

  return "local-mock-relay-daemon";
}

function runtimeRecoveryPolicy(engine: TransportEngineKind): TransportRuntimeSession["recoveryPolicy"] {
  return engine === "nativePreview" ? "auto" : "manual";
}

function runtimeAdapterKind(engine: TransportEngineKind): TransportRuntimeSession["adapterKind"] {
  return engine === "nativePreview" ? "localCommand" : "embedded";
}

function runtimeLaunchStatus(engine: TransportEngineKind): TransportRuntimeSession["launchStatus"] {
  return engine === "nativePreview" ? "unknown" : "embedded";
}

function runtimeSessionPrefix(engine: TransportEngineKind) {
  return engine === "nativePreview" ? "native" : "mock";
}

function runtimeProtocolToken(
  engine: TransportEngineKind,
  protocol: CircleTransportDiagnostic["protocol"],
  useTorNetwork: boolean,
) {
  if (engine === "nativePreview" && protocol === "websocket" && useTorNetwork) {
    return "tor-ws";
  }

  return protocol === "mesh" ? "mesh" : protocol === "invite" ? "invite" : "ws";
}

function runtimeEndpointSegment(protocol: CircleTransportDiagnostic["protocol"]) {
  return protocol === "mesh" ? "mesh" : protocol === "invite" ? "invite" : "relay";
}

function runtimeEndpointScheme(
  engine: TransportEngineKind,
  protocol: CircleTransportDiagnostic["protocol"],
  useTorNetwork: boolean,
) {
  if (engine === "nativePreview" && protocol === "websocket" && useTorNetwork) {
    return "native+tor";
  }

  return engine === "nativePreview" ? "native" : "loopback";
}

function runtimeLaunchCommand(engine: TransportEngineKind) {
  return engine === "nativePreview" ? "p2p-chat-runtime" : undefined;
}

function runtimeLaunchArguments(
  engine: TransportEngineKind,
  protocol: CircleTransportDiagnostic["protocol"],
  useTorNetwork: boolean,
  circleId: string,
) {
  if (engine !== "nativePreview") {
    return [] as string[];
  }

  if (protocol === "mesh") {
    return ["preview-mesh", "--circle", circleId];
  }

  if (protocol === "invite") {
    return ["preview-invite", "--circle", circleId];
  }

  return useTorNetwork
    ? ["preview-relay", "--tor", "--circle", circleId]
    : ["preview-relay", "--circle", circleId];
}

function runtimeActionEvent(
  engine: TransportEngineKind,
  action: TransportCircleAction,
) {
  if (engine === "nativePreview") {
    return action === "connect"
      ? "native runtime booted"
      : action === "disconnect"
        ? "native runtime released"
        : action === "discoverPeers"
          ? "native discovery sweep committed"
          : action === "sync"
            ? "native relay checkpoint committed"
            : "native session merge committed";
  }

  return action === "connect"
    ? "mock runtime handshake enqueued"
    : action === "disconnect"
      ? "mock runtime released"
      : action === "discoverPeers"
        ? "mock peer sweep queued"
        : action === "sync"
          ? "mock relay checkpoint synced"
          : "mock session merge queued";
}

function runtimeStatusEvent(
  engine: TransportEngineKind,
  status: CircleItem["status"],
) {
  if (engine === "nativePreview") {
    return status === "open"
      ? "native runtime active"
      : status === "connecting"
        ? "native runtime booting"
        : "native runtime idle";
  }

  return status === "open"
    ? "mock runtime active"
    : status === "connecting"
      ? "mock runtime booting"
      : "mock runtime idle";
}

function initialRuntimeSince(state: TransportRuntimeSession["state"]) {
  return runtimeIsLive(state) ? "this launch" : "not started";
}

function runtimeDesiredState(args: {
  previous?: TransportRuntimeSession;
  state: TransportRuntimeSession["state"];
  pendingAction?: TransportCircleAction;
}) {
  if (args.pendingAction === "disconnect") {
    return "stopped" satisfies TransportRuntimeSession["desiredState"];
  }

  if (args.pendingAction) {
    return "running" satisfies TransportRuntimeSession["desiredState"];
  }

  if (runtimeIsLive(args.state)) {
    return "running" satisfies TransportRuntimeSession["desiredState"];
  }

  return args.previous?.desiredState ?? ("stopped" satisfies TransportRuntimeSession["desiredState"]);
}

function runtimeRetryLabel(restartAttempts: number) {
  if (restartAttempts <= 1) {
    return 3_000;
  }

  if (restartAttempts === 2) {
    return 10_000;
  }

  if (restartAttempts === 3) {
    return 30_000;
  }

  return 60_000;
}

function runtimeRetryCountdownLabel(remainingMs: number) {
  const remainingSeconds = Math.max(1, Math.ceil(remainingMs / 1000));
  if (remainingSeconds < 60) {
    return `in ${remainingSeconds}s`;
  }

  return `in ${Math.ceil(remainingSeconds / 60)}m`;
}

function runtimeFailureReason(
  engine: TransportEngineKind,
  previousState: TransportRuntimeSession["state"],
) {
  if (engine === "nativePreview") {
    return previousState === "starting"
      ? "native preview runtime dropped during startup"
      : "native preview runtime heartbeat expired";
  }

  return previousState === "starting"
    ? "local runtime dropped during startup"
    : "local runtime heartbeat expired";
}

function runtimeRecoveryQueue(args: {
  previous?: TransportRuntimeSession;
  state: TransportRuntimeSession["state"];
  desiredState: TransportRuntimeSession["desiredState"];
  recoveryPolicy: TransportRuntimeSession["recoveryPolicy"];
  identityChanged: boolean;
  nowMs: number;
}): Pick<
  TransportRuntimeSession,
  "queueState" | "restartAttempts" | "nextRetryIn" | "nextRetryAtMs"
> {
  const previous = args.identityChanged ? undefined : args.previous;

  if (
    args.desiredState === "stopped" ||
    runtimeIsLive(args.state) ||
    args.recoveryPolicy === "manual"
  ) {
    return {
      queueState: "idle" satisfies TransportRuntimeSession["queueState"],
      restartAttempts: 0,
      nextRetryIn: undefined,
      nextRetryAtMs: undefined,
    };
  }

  const previousAttempts = previous?.restartAttempts ?? 0;
  const failureDetected = !!previous && runtimeIsLive(previous.state);
  const restartAttempts = failureDetected ? Math.max(previousAttempts + 1, 1) : previousAttempts;
  const nextRetryAtMs = failureDetected
    ? args.nowMs + runtimeRetryLabel(restartAttempts)
    : previous?.nextRetryAtMs ??
      (restartAttempts > 0 ? args.nowMs + runtimeRetryLabel(restartAttempts) : args.nowMs);
  const queueState: TransportRuntimeSession["queueState"] =
    nextRetryAtMs > args.nowMs ? "backoff" : "queued";

  return {
    queueState,
    restartAttempts,
    nextRetryIn:
      queueState === "queued"
        ? "when local runtime worker is ready"
        : runtimeRetryCountdownLabel(nextRetryAtMs - args.nowMs),
    nextRetryAtMs,
  };
}

function runtimeFailureState(args: {
  previous?: TransportRuntimeSession;
  state: TransportRuntimeSession["state"];
  desiredState: TransportRuntimeSession["desiredState"];
  identityChanged: boolean;
  engine: TransportEngineKind;
}): Pick<TransportRuntimeSession, "lastFailureReason" | "lastFailureAt"> {
  const previous = args.identityChanged ? undefined : args.previous;

  if (args.desiredState === "stopped" || runtimeIsLive(args.state)) {
    return {
      lastFailureReason: undefined,
      lastFailureAt: undefined,
    };
  }

  const failureDetected = !!previous && runtimeIsLive(previous.state);
  if (failureDetected) {
    return {
      lastFailureReason: runtimeFailureReason(args.engine, previous.state),
      lastFailureAt: "now",
    };
  }

  return {
    lastFailureReason: previous?.lastFailureReason,
    lastFailureAt: previous?.lastFailureAt,
  };
}

function runtimeGeneration(args: {
  previous?: TransportRuntimeSession;
  state: TransportRuntimeSession["state"];
  bootStarted: boolean;
}) {
  if (!args.previous) {
    return runtimeIsLive(args.state) ? 1 : 0;
  }

  if (args.bootStarted) {
    return args.previous.generation === 0 ? 1 : args.previous.generation + 1;
  }

  if (runtimeIsLive(args.state)) {
    return Math.max(args.previous.generation, 1);
  }

  return args.previous.generation;
}

function buildFallbackRuntimeSessions(args: {
  circles: CircleItem[];
  advanced: AdvancedPreferences;
  previousSnapshot?: TransportSnapshot | null;
  pendingActivity?: { circleId: string; action: TransportCircleAction };
}) {
  const engine = transportEngineFromAdvanced(args.advanced);
  const nowMs = Date.now();
  const previousSessionIndex = new Map(
    (args.previousSnapshot?.runtimeSessions ?? []).map((session) => [session.circleId, session]),
  );

  return args.circles.map((circle) => {
    const protocol = protocolFromRelay(circle.relay);
    const state = runtimeStateFromCircleStatus(circle.status);
    const driver = runtimeDriver(engine, protocol, args.advanced.useTorNetwork);
    const adapterKind = runtimeAdapterKind(engine);
    const launchStatus = runtimeLaunchStatus(engine);
    const launchCommand = runtimeLaunchCommand(engine);
    const launchArguments = runtimeLaunchArguments(
      engine,
      protocol,
      args.advanced.useTorNetwork,
      circle.id,
    );
    const recoveryPolicy = runtimeRecoveryPolicy(engine);
    const sessionLabel = `${runtimeSessionPrefix(engine)}::${runtimeProtocolToken(
      engine,
      protocol,
      args.advanced.useTorNetwork,
    )}::${circle.id}`;
    const endpoint = `${runtimeEndpointScheme(
      engine,
      protocol,
      args.advanced.useTorNetwork,
    )}://${runtimeEndpointSegment(protocol)}/${circle.id}`;
    const previous = previousSessionIndex.get(circle.id);
    const pendingAction =
      args.pendingActivity?.circleId === circle.id ? args.pendingActivity.action : undefined;
    const identityChanged = !!previous && (
      previous.driver !== driver ||
      previous.adapterKind !== adapterKind ||
      previous.launchStatus !== launchStatus ||
      previous.launchCommand !== launchCommand ||
      JSON.stringify(previous.launchArguments) !== JSON.stringify(launchArguments) ||
      previous.sessionLabel !== sessionLabel ||
      previous.endpoint !== endpoint
    );
    const stateChanged = !!previous && previous.state !== state;
    const bootStarted =
      pendingAction === "connect" ||
      (!previous ? runtimeIsLive(state) : !runtimeIsLive(previous.state) && runtimeIsLive(state)) ||
      (identityChanged && runtimeIsLive(state));
    const desiredState = runtimeDesiredState({
      previous,
      state,
      pendingAction,
    });
    const generation = runtimeGeneration({
      previous,
      state,
      bootStarted,
    });
    const queue = runtimeRecoveryQueue({
      previous,
      state,
      desiredState,
      recoveryPolicy,
      identityChanged,
      nowMs,
    });
    const failure = runtimeFailureState({
      previous,
      state,
      desiredState,
      identityChanged,
      engine,
    });

    return {
      circleId: circle.id,
      driver,
      adapterKind,
      launchStatus,
      launchCommand,
      launchArguments,
      resolvedLaunchCommand: launchCommand,
      launchError:
        launchStatus === "unknown"
          ? "Browser preview cannot verify whether the local runtime command is installed."
          : undefined,
      desiredState,
      recoveryPolicy,
      queueState: queue.queueState,
      restartAttempts: queue.restartAttempts,
      nextRetryIn: queue.nextRetryIn,
      nextRetryAtMs: queue.nextRetryAtMs,
      lastFailureReason: failure.lastFailureReason,
      lastFailureAt: failure.lastFailureAt,
      state,
      generation,
      stateSince:
        identityChanged || stateChanged
          ? "now"
          : previous?.stateSince ?? initialRuntimeSince(state),
      sessionLabel,
      endpoint,
      lastEvent: pendingAction
        ? runtimeActionEvent(engine, pendingAction)
        : identityChanged || stateChanged
          ? runtimeStatusEvent(engine, circle.status)
          : previous?.lastEvent ?? runtimeStatusEvent(engine, circle.status),
      lastEventAt:
        pendingAction || identityChanged || stateChanged
          ? "now"
          : previous?.lastEventAt ?? initialRuntimeSince(state),
    } satisfies TransportRuntimeSession;
  });
}

function mergeFallbackActivities(
  previousActivities: TransportActivityItem[],
  diagnostics: CircleTransportDiagnostic[],
  engine: TransportEngineKind,
  runtimeSessions: TransportRuntimeSession[],
  previousRuntimeSessions: TransportRuntimeSession[],
  pendingActivity?: { circleId: string; action: TransportCircleAction },
) {
  const validCircleIds = new Set(diagnostics.map((item) => item.circleId));
  const activities = previousActivities
    .filter((item) => validCircleIds.has(item.circleId))
    .map((item) => cloneState(item));

  for (const diagnostic of diagnostics) {
    const runtimeActivity = buildRuntimeActivity(engine, diagnostic);
    const runtimeIndex = activities.findIndex((item) => {
      return item.circleId === diagnostic.circleId && item.kind === "runtime";
    });

    if (runtimeIndex >= 0) {
      activities[runtimeIndex] = runtimeActivity;
    } else {
      activities.push(runtimeActivity);
    }
  }

  if (pendingActivity) {
    const diagnostic = diagnostics.find((item) => item.circleId === pendingActivity.circleId);
    if (diagnostic) {
      activities.unshift(buildActionActivity(engine, diagnostic, pendingActivity.action));
    }
  }

  return mergeRuntimeSessionActivities(
    trimTransportActivities(activities),
    runtimeSessions,
    previousRuntimeSessions,
  );
}

function deriveFallbackSnapshot(args: {
  circles: CircleItem[];
  contacts: ContactItem[];
  sessions: SessionItem[];
  groups: GroupProfile[];
  messageStore: Record<string, MessageItem[]>;
  activeCircleId: string;
  advanced: AdvancedPreferences;
  previousSnapshot?: TransportSnapshot | null;
  pendingActivity?: { circleId: string; action: TransportCircleAction };
}): TransportSnapshot {
  const engine = transportEngineFromAdvanced(args.advanced);
  const diagnostics = args.circles.map((circle) => {
    const sessionsForCircle = args.sessions.filter((session) => session.circleId === circle.id);
    const protocol = protocolFromRelay(circle.relay);
    const health = healthFromStatus(circle.status);

    return {
      circleId: circle.id,
      relay: circle.relay,
      protocol,
      health,
      latencyMs: parseLatencyMs(circle.latency),
      peerCount:
        circle.status === "closed"
          ? 0
          : Math.max(1, sessionsForCircle.length * 2) + (args.advanced.experimentalTransport ? 1 : 0),
      queuedMessages:
        circle.status === "connecting"
          ? Math.min(3, sessionsForCircle.length)
          : args.advanced.experimentalTransport && circle.status === "open"
            ? 0
            : 0,
      lastSync:
        args.advanced.experimentalTransport
          ? previewLastSync(protocol, health)
          : circle.status === "open"
            ? "just now"
            : circle.status === "connecting"
              ? "pending handshake"
              : "offline",
      reachable: circle.status !== "closed",
    } satisfies CircleTransportDiagnostic;
  });
  const groupIndex = new Map(args.groups.map((group) => [group.sessionId, group]));
  const contactIndex = new Map(args.contacts.map((contact) => [contact.id, contact]));
  const peerSessions = new Map<string, number>();

  for (const session of args.sessions) {
    if (session.contactId) {
      const key = `${session.circleId}:${session.contactId}`;
      peerSessions.set(key, (peerSessions.get(key) ?? 0) + 1);
    }

    if (session.kind === "group") {
      const group = groupIndex.get(session.id);
      if (!group) {
        continue;
      }

      for (const member of group.members) {
        const key = `${session.circleId}:${member.contactId}`;
        peerSessions.set(key, (peerSessions.get(key) ?? 0) + 1);
      }
    }
  }

  const peers = [...peerSessions.entries()]
    .map(([key, sharedSessions]) => {
      const [circleId, contactId] = key.split(":");
      const contact = contactIndex.get(contactId);
      const diagnostic = diagnostics.find((item) => item.circleId === circleId);
      if (!contact || !diagnostic) {
        return null;
      }

      return {
        circleId,
        contactId,
        name: contact.name,
        handle: contact.handle,
        presence: contact.blocked
          ? "offline"
          : contact.online
            ? "online"
            : "idle",
        route:
          args.advanced.experimentalTransport
            ? labelWithNativeSuffix(
                diagnostic.protocol === "mesh"
                  ? "mesh hop"
                  : diagnostic.protocol === "invite"
                    ? "invite handoff"
                    : "direct relay",
              )
            : diagnostic.protocol === "mesh"
            ? "mesh hop"
            : diagnostic.protocol === "invite"
              ? "invite handoff"
              : "direct relay",
        sharedSessions,
        lastSeen: contact.blocked
          ? "blocked"
          : args.advanced.experimentalTransport
            ? contact.online
              ? "native session active"
              : "native standby"
            : contact.online
              ? "now"
              : "recently",
        blocked: !!contact.blocked,
      } satisfies DiscoveredPeer;
    })
    .filter((item): item is DiscoveredPeer => !!item);

  const sessionSync = args.sessions.map((session) => {
    const diagnostic = diagnostics.find((item) => item.circleId === session.circleId);
    const pendingMessages = session.unreadCount ?? 0;

    return {
      circleId: session.circleId,
      sessionId: session.id,
      sessionName: session.name,
      state:
        pendingMessages > 0
          ? "pending"
          : diagnostic?.health === "degraded"
            ? "syncing"
            : session.kind === "group" && (session.members ?? 0) >= 10
              ? "conflict"
              : "idle",
      pendingMessages,
      source:
        args.advanced.experimentalTransport
          ? labelWithNativeSuffix(
              diagnostic?.protocol === "mesh"
                ? "mesh hop"
                : diagnostic?.protocol === "invite"
                  ? "invite handoff"
                  : "direct relay",
            )
          : diagnostic?.protocol === "mesh"
            ? "mesh hop"
            : diagnostic?.protocol === "invite"
              ? "invite handoff"
              : "direct relay",
      lastMerge:
        diagnostic?.health === "offline"
          ? "offline"
          : pendingMessages > 0
            ? "pending merge"
            : args.advanced.experimentalTransport
              ? "native checkpoint"
              : session.time,
    } satisfies SessionSyncItem;
  });
  const runtimeSessions = buildFallbackRuntimeSessions({
    circles: args.circles,
    advanced: args.advanced,
    previousSnapshot: args.previousSnapshot,
    pendingActivity: args.pendingActivity,
  });
  const activities = mergeFallbackActivities(
    args.previousSnapshot?.activities ?? [],
    diagnostics,
    engine,
    runtimeSessions,
    args.previousSnapshot?.runtimeSessions ?? [],
    args.pendingActivity,
  );

  return {
    engine,
    status: diagnostics.some((item) => item.health === "online")
      ? "online"
      : diagnostics.some((item) => item.health === "degraded")
        ? "degraded"
        : "offline",
    activeCircleId:
      diagnostics.find((item) => item.circleId === args.activeCircleId)?.circleId ??
      diagnostics[0]?.circleId ??
      "",
    relayCount: diagnostics.length,
    connectedRelays: diagnostics.filter((item) => item.health === "online").length,
    queuedMessages: diagnostics.reduce((sum, item) => sum + item.queuedMessages, 0),
    capabilities: {
      supportsMesh: diagnostics.some((item) => item.protocol === "mesh"),
      supportsPaidRelays: args.circles.some((circle) => circle.type === "paid"),
      supportsTor: args.advanced.useTorNetwork,
      experimentalEnabled: args.advanced.experimentalTransport,
    },
    diagnostics,
    peers,
    sessionSync,
    activities,
    runtimeSessions,
  };
}

export function loadTransportSnapshotLocally(fallback: {
  circles: CircleItem[];
  contacts: ContactItem[];
  sessions: SessionItem[];
  groups: GroupProfile[];
  messageStore: Record<string, MessageItem[]>;
  activeCircleId: string;
  advanced: AdvancedPreferences;
  previousSnapshot?: TransportSnapshot | null;
  pendingActivity?: { circleId: string; action: TransportCircleAction };
}): TransportSnapshot {
  return deriveFallbackSnapshot(fallback);
}

export async function loadTransportSnapshot(
  input: TransportSnapshotInput,
  fallback: {
    circles: CircleItem[];
    contacts: ContactItem[];
    sessions: SessionItem[];
    groups: GroupProfile[];
    messageStore: Record<string, MessageItem[]>;
    activeCircleId: string;
    advanced: AdvancedPreferences;
    previousSnapshot?: TransportSnapshot | null;
    pendingActivity?: { circleId: string; action: TransportCircleAction };
  },
): Promise<TransportSnapshotLoadResult> {
  if (!hasTauriRuntime()) {
    return {
      snapshot: loadTransportSnapshotLocally(fallback),
      source: "fallback",
    };
  }

  const snapshot = await invoke<TransportSnapshot>("load_transport_snapshot", {
    input: cloneState(input),
  });
  return {
    snapshot: cloneState(snapshot),
    source: "tauri",
  };
}

export function deriveRuntimeRetryAction(
  circle: CircleItem,
  session: TransportRuntimeSession,
): TransportCircleAction | null {
  if (
    session.circleId !== circle.id ||
    session.desiredState !== "running" ||
    session.launchStatus === "missing"
  ) {
    return null;
  }

  if (circle.status === "connecting" && session.state === "starting") {
    return "sync";
  }

  if (
    session.state === "inactive" ||
    session.queueState !== "idle" ||
    !!session.lastFailureReason
  ) {
    return "connect";
  }

  return null;
}

export function deriveCircleRuntimeRetryAction(
  circle: CircleItem,
  runtimeSessions: TransportRuntimeSession[],
): TransportCircleAction | null {
  for (const session of runtimeSessions) {
    const action = deriveRuntimeRetryAction(circle, session);
    if (action) {
      return action;
    }
  }

  return null;
}

export function deriveRuntimeRecoveryAction(
  snapshot: TransportSnapshot,
  circles: CircleItem[],
): { circleId: string; action: TransportCircleAction } | null {
  const nowMs = Date.now();
  for (const session of snapshot.runtimeSessions) {
    if (session.desiredState !== "running" || session.recoveryPolicy !== "auto") {
      continue;
    }

    const circle = circles.find((item) => item.id === session.circleId);
    if (!circle) {
      continue;
    }

    const action = deriveRuntimeRetryAction(circle, session);
    if (action === "sync") {
      return {
        circleId: session.circleId,
        action,
      };
    }

    if (
      action === "connect" &&
      circle.status === "closed" &&
      session.nextRetryAtMs !== undefined &&
      session.nextRetryAtMs <= nowMs
    ) {
      return {
        circleId: session.circleId,
        action,
      };
    }
  }

  return null;
}

export async function applyTransportCircleAction(
  input: TransportCircleActionInput,
): Promise<
  | {
      kind: "applied";
      result: TransportMutationResult;
    }
  | {
      kind: "soft-fallback";
    }
  | {
      kind: "blocked";
      code: "runtimeLaunchMissing";
      message: string;
    }
> {
  if (!hasTauriRuntime()) {
    return {
      kind: "soft-fallback",
    };
  }

  try {
    const result = await invoke<TransportMutationResult>("apply_transport_circle_action", {
      input: cloneState(input),
    });
    return {
      kind: "applied",
      result: cloneState(result),
    };
  } catch (error) {
    const message = String(error ?? "");
    if (message.includes("transport_runtime_launch_missing:")) {
      return {
        kind: "blocked",
        code: "runtimeLaunchMissing",
        message: message.replace("transport_runtime_launch_missing:", "").trim(),
      };
    }

    throw error;
  }
}
