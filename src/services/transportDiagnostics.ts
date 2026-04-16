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
  TransportSnapshot,
  TransportSnapshotInput,
} from "../types/chat";

function cloneState<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
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

function mergeFallbackActivities(
  previousActivities: TransportActivityItem[],
  diagnostics: CircleTransportDiagnostic[],
  engine: TransportEngineKind,
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

  return trimTransportActivities(activities);
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
  const activities = mergeFallbackActivities(
    args.previousSnapshot?.activities ?? [],
    diagnostics,
    engine,
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
  };
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
): Promise<TransportSnapshot> {
  try {
    const snapshot = await invoke<TransportSnapshot>("load_transport_snapshot", {
      input: cloneState(input),
    });
    return cloneState(snapshot);
  } catch {
    return deriveFallbackSnapshot(fallback);
  }
}

export async function applyTransportCircleAction(
  input: TransportCircleActionInput,
): Promise<TransportMutationResult | null> {
  try {
    const result = await invoke<TransportMutationResult>("apply_transport_circle_action", {
      input: cloneState(input),
    });
    return cloneState(result);
  } catch {
    return null;
  }
}
