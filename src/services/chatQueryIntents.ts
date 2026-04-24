export type ChatQueryKind = "relay" | "invite" | "handle" | "pubkey" | "lookup";

const RELAY_QUERY_PATTERN = /^(wss?:\/\/|mesh:\/\/)/i;
const INVITE_QUERY_PATTERN =
  /^(invite:\/\/|p2pchat:\/\/|circle:\/\/|xchat:\/\/|https?:\/\/)/i;
const ACCOUNT_SCHEME_PATTERN = /^(bunker:\/\/|nostrconnect:\/\/)/i;
const HEX_PUBKEY_PATTERN = /^(?:0x)?[a-f0-9]{32,}$/i;
const PUBLIC_RELAY_SHORTCUTS = new Set(["0xchat", "damus", "nos", "primal", "yabu", "nostrband"]);

export function classifyChatQuery(query: string): ChatQueryKind | null {
  const trimmed = query.trim();
  if (!trimmed) {
    return null;
  }

  if (RELAY_QUERY_PATTERN.test(trimmed)) {
    return "relay";
  }

  if (PUBLIC_RELAY_SHORTCUTS.has(trimmed.toLowerCase())) {
    return "relay";
  }

  if (trimmed.startsWith("@")) {
    return "handle";
  }

  if (trimmed.startsWith("npub") || HEX_PUBKEY_PATTERN.test(trimmed)) {
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
