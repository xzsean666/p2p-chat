export type ChatQueryKind = "relay" | "invite" | "handle" | "pubkey" | "lookup";

const RELAY_QUERY_PATTERN = /^(wss?:\/\/|mesh:\/\/)/i;
const INVITE_QUERY_PATTERN =
  /^(invite:\/\/|p2pchat:\/\/|circle:\/\/|xchat:\/\/|https?:\/\/)/i;
const ACCOUNT_SCHEME_PATTERN = /^(bunker:\/\/|nostrconnect:\/\/)/i;

export function classifyChatQuery(query: string): ChatQueryKind | null {
  const trimmed = query.trim();
  if (!trimmed) {
    return null;
  }

  if (RELAY_QUERY_PATTERN.test(trimmed)) {
    return "relay";
  }

  if (trimmed.startsWith("@")) {
    return "handle";
  }

  if (trimmed.startsWith("npub") || /^[a-f0-9]{32,}$/i.test(trimmed)) {
    return "pubkey";
  }

  if (INVITE_QUERY_PATTERN.test(trimmed)) {
    return "invite";
  }

  if (trimmed.includes("://") && !ACCOUNT_SCHEME_PATTERN.test(trimmed)) {
    return "invite";
  }

  return "lookup";
}

export function isCircleQuery(query: string): boolean {
  const kind = classifyChatQuery(query);
  return kind === "relay" || kind === "invite";
}
