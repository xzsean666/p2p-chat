import type { SettingPageId } from "../../types/chat";

export type OverlayPage =
  | { kind: "circle-directory" }
  | { kind: "circle-detail"; circleId: string }
  | { kind: "settings-detail"; settingId: SettingPageId }
  | { kind: "message-detail"; sessionId: string; messageId: string }
  | { kind: "new-message" }
  | { kind: "circle-invite" }
  | { kind: "self-chat-confirm" }
  | { kind: "group-select-members" }
  | { kind: "group-create"; memberContactIds: string[] }
  | { kind: "find-people"; mode?: "chat" | "join-circle" }
  | { kind: "archived" }
  | { kind: "contact"; contactId: string }
  | { kind: "group"; sessionId: string }
  | { kind: "group-name"; sessionId: string }
  | { kind: "group-members"; sessionId: string }
  | { kind: "group-add-members"; sessionId: string }
  | { kind: "group-remove-members"; sessionId: string };

export interface OverlayHistoryState {
  overlayPages: OverlayPage[];
  overlayDepth: number;
}

const settingPageIds = new Set<SettingPageId>([
  "preferences",
  "notifications",
  "advanced",
  "restore",
  "about",
]);

function isSettingPageId(value: string): value is SettingPageId {
  return settingPageIds.has(value as SettingPageId);
}

function decodeRouteSegment(value: string) {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function parseOverlayPageRecord(value: unknown): OverlayPage | null {
  if (!value || typeof value !== "object") {
    return null;
  }

  const record = value as Record<string, unknown>;
  const kind = typeof record.kind === "string" ? record.kind : "";

  switch (kind) {
    case "circle-directory":
    case "new-message":
    case "circle-invite":
    case "self-chat-confirm":
    case "group-select-members":
    case "archived":
      return { kind };
    case "find-people":
      return record.mode === "join-circle"
        ? { kind, mode: "join-circle" }
        : { kind, mode: "chat" };
    case "circle-detail":
      return typeof record.circleId === "string"
        ? { kind, circleId: record.circleId }
        : null;
    case "group-create":
      return Array.isArray(record.memberContactIds) &&
        record.memberContactIds.every((item) => typeof item === "string")
        ? {
            kind,
            memberContactIds: record.memberContactIds as string[],
          }
        : null;
    case "settings-detail":
      return typeof record.settingId === "string" && isSettingPageId(record.settingId)
        ? { kind, settingId: record.settingId }
        : null;
    case "message-detail":
      return typeof record.sessionId === "string" && typeof record.messageId === "string"
        ? { kind, sessionId: record.sessionId, messageId: record.messageId }
        : null;
    case "contact":
      return typeof record.contactId === "string"
        ? { kind, contactId: record.contactId }
        : null;
    case "group":
    case "group-name":
    case "group-members":
    case "group-add-members":
    case "group-remove-members":
      return typeof record.sessionId === "string"
        ? { kind, sessionId: record.sessionId }
        : null;
    default:
      return null;
  }
}

export function cloneOverlayPages(pages: OverlayPage[]): OverlayPage[] {
  return pages.map((page) => ({ ...page }));
}

function encodeOverlayPageStack(pages: OverlayPage[]) {
  return encodeURIComponent(JSON.stringify(cloneOverlayPages(pages)));
}

function decodeOverlayPageStack(value: string): OverlayPage[] {
  try {
    const parsed = JSON.parse(decodeURIComponent(value));
    if (!Array.isArray(parsed)) {
      return [];
    }

    return parsed.flatMap((item) => {
      const page = parseOverlayPageRecord(item);
      return page ? [page] : [];
    });
  } catch {
    return [];
  }
}

export function overlayRouteHash(pages: OverlayPage[]): string {
  if (pages.length > 1) {
    return `#/stack/${encodeOverlayPageStack(pages)}`;
  }

  const page = pages[pages.length - 1];
  if (!page) {
    return "";
  }

  switch (page.kind) {
    case "circle-directory":
      return "#/circles";
    case "circle-detail":
      return `#/circles/${encodeURIComponent(page.circleId)}`;
    case "settings-detail":
      return `#/settings/${encodeURIComponent(page.settingId)}`;
    case "message-detail":
      return `#/messages/${encodeURIComponent(page.sessionId)}/${encodeURIComponent(page.messageId)}`;
    case "new-message":
      return "#/new-message";
    case "circle-invite":
      return "#/new-message/invite";
    case "self-chat-confirm":
      return "#/new-message/self";
    case "group-select-members":
      return "#/new-group/select";
    case "group-create":
      return `#/new-group/create/${encodeURIComponent(JSON.stringify(page.memberContactIds))}`;
    case "find-people":
      return page.mode === "join-circle" ? "#/find-people/join-circle" : "#/find-people";
    case "archived":
      return "#/archived";
    case "contact":
      return `#/contacts/${encodeURIComponent(page.contactId)}`;
    case "group":
      return `#/groups/${encodeURIComponent(page.sessionId)}`;
    case "group-name":
      return `#/groups/${encodeURIComponent(page.sessionId)}/name`;
    case "group-members":
      return `#/groups/${encodeURIComponent(page.sessionId)}/members`;
    case "group-add-members":
      return `#/groups/${encodeURIComponent(page.sessionId)}/add-members`;
    case "group-remove-members":
      return `#/groups/${encodeURIComponent(page.sessionId)}/remove-members`;
  }
}

export function parseOverlayRouteHash(hash: string): OverlayPage[] {
  const normalizedHash = hash.startsWith("#") ? hash.slice(1) : hash;
  const normalizedPath = normalizedHash.replace(/^\/+/, "");
  if (!normalizedPath) {
    return [] as OverlayPage[];
  }

  if (normalizedPath.startsWith("stack/")) {
    return decodeOverlayPageStack(normalizedPath.slice("stack/".length));
  }

  const segments = normalizedPath.split("/").filter(Boolean).map(decodeRouteSegment);
  if (!segments.length) {
    return [] as OverlayPage[];
  }

  if (segments[0] === "circles" && segments.length === 1) {
    return [{ kind: "circle-directory" }];
  }

  if (segments[0] === "circles" && segments[1]) {
    return [{ kind: "circle-detail", circleId: segments[1] }];
  }

  if (segments[0] === "settings" && segments[1] && isSettingPageId(segments[1])) {
    return [{ kind: "settings-detail", settingId: segments[1] }];
  }

  if (segments[0] === "messages" && segments[1] && segments[2]) {
    return [{ kind: "message-detail", sessionId: segments[1], messageId: segments[2] }];
  }

  if (segments[0] === "new-message" && segments[1] === "invite") {
    return [{ kind: "circle-invite" }];
  }

  if (segments[0] === "new-message" && segments[1] === "self") {
    return [{ kind: "self-chat-confirm" }];
  }

  if (segments[0] === "new-message") {
    return [{ kind: "new-message" }];
  }

  if (segments[0] === "new-group" && segments[1] === "select") {
    return [{ kind: "group-select-members" }];
  }

  if (segments[0] === "new-group" && segments[1] === "create" && segments[2]) {
    try {
      const memberContactIds = JSON.parse(segments[2]);
      if (Array.isArray(memberContactIds) && memberContactIds.every((item) => typeof item === "string")) {
        return [{ kind: "group-create", memberContactIds }];
      }
    } catch {
      return [] as OverlayPage[];
    }
  }

  if (segments[0] === "find-people") {
    return [{ kind: "find-people", mode: segments[1] === "join-circle" ? "join-circle" : "chat" }];
  }

  if (segments[0] === "archived") {
    return [{ kind: "archived" }];
  }

  if (segments[0] === "contacts" && segments[1]) {
    return [{ kind: "contact", contactId: segments[1] }];
  }

  if (segments[0] === "groups" && segments[1]) {
    if (segments[2] === "name") {
      return [{ kind: "group-name", sessionId: segments[1] }];
    }

    if (segments[2] === "members") {
      return [{ kind: "group-members", sessionId: segments[1] }];
    }

    if (segments[2] === "add-members") {
      return [{ kind: "group-add-members", sessionId: segments[1] }];
    }

    if (segments[2] === "remove-members") {
      return [{ kind: "group-remove-members", sessionId: segments[1] }];
    }

    return [{ kind: "group", sessionId: segments[1] }];
  }

  return [] as OverlayPage[];
}

export function parseOverlayPagesFromHistoryState(
  value: unknown,
): OverlayPage[] | null {
  return parseOverlayHistoryState(value)?.overlayPages ?? null;
}

export function parseOverlayHistoryState(
  value: unknown,
): OverlayHistoryState | null {
  if (!value || typeof value !== "object") {
    return null;
  }

  const record = value as Record<string, unknown>;
  if (!Array.isArray(record.overlayPages)) {
    return null;
  }

  const overlayPages = record.overlayPages.flatMap((item) => {
    const page = parseOverlayPageRecord(item);
    return page ? [page] : [];
  });

  const overlayDepth =
    typeof record.overlayDepth === "number" &&
    Number.isInteger(record.overlayDepth) &&
    record.overlayDepth >= 0
      ? record.overlayDepth
      : overlayPages.length;

  return {
    overlayPages,
    overlayDepth,
  };
}

export function createOverlayHistoryState(
  pages: OverlayPage[],
  overlayDepth = pages.length,
): OverlayHistoryState {
  return {
    overlayPages: cloneOverlayPages(pages),
    overlayDepth:
      Number.isInteger(overlayDepth) && overlayDepth >= 0 ? overlayDepth : pages.length,
  };
}
