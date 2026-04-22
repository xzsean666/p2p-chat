import type { MessageItem, MessageReplyPreview, SessionItem } from "../../types/chat";

type MessageAuthorSession = Pick<SessionItem, "kind" | "name" | "initials"> | null | undefined;

const BECH32_CHARSET = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";
const BECH32_GENERATORS = [
  0x3b6a57b2,
  0x26508e6d,
  0x1ea119fa,
  0x3d4233dd,
  0x2a1462b3,
] as const;

function normalizedLabel(value?: string | null) {
  const trimmed = value?.trim() ?? "";
  return trimmed || "";
}

function bech32Polymod(values: number[]) {
  let checksum = 1;
  for (const value of values) {
    const top = checksum >>> 25;
    checksum = ((checksum & 0x1ffffff) << 5) ^ value;
    for (let index = 0; index < BECH32_GENERATORS.length; index += 1) {
      if ((top >>> index) & 1) {
        checksum ^= BECH32_GENERATORS[index];
      }
    }
  }

  return checksum;
}

function bech32HrpExpand(prefix: string) {
  const expanded = [];
  for (let index = 0; index < prefix.length; index += 1) {
    expanded.push(prefix.charCodeAt(index) >>> 5);
  }
  expanded.push(0);
  for (let index = 0; index < prefix.length; index += 1) {
    expanded.push(prefix.charCodeAt(index) & 31);
  }
  return expanded;
}

function bech32Decode(value: string) {
  const normalized = value.trim().toLowerCase();
  const separatorIndex = normalized.lastIndexOf("1");
  if (separatorIndex <= 0 || separatorIndex + 7 > normalized.length) {
    return null;
  }

  const prefix = normalized.slice(0, separatorIndex);
  const payload = normalized.slice(separatorIndex + 1);
  const data = Array.from(payload, (character) => BECH32_CHARSET.indexOf(character));
  if (data.some((valueIndex) => valueIndex < 0)) {
    return null;
  }

  const checksum = bech32Polymod([...bech32HrpExpand(prefix), ...data]);
  if (checksum !== 1) {
    return null;
  }

  return {
    prefix,
    data: data.slice(0, -6),
  };
}

function convertBits(data: number[], fromBits: number, toBits: number, pad: boolean) {
  let accumulator = 0;
  let bits = 0;
  const converted = [];
  const maxValue = (1 << toBits) - 1;

  for (const value of data) {
    if (value < 0 || value >>> fromBits !== 0) {
      return null;
    }

    accumulator = (accumulator << fromBits) | value;
    bits += fromBits;
    while (bits >= toBits) {
      bits -= toBits;
      converted.push((accumulator >>> bits) & maxValue);
    }
  }

  if (pad) {
    if (bits > 0) {
      converted.push((accumulator << (toBits - bits)) & maxValue);
    }
  } else if (bits >= fromBits || ((accumulator << (toBits - bits)) & maxValue) !== 0) {
    return null;
  }

  return converted;
}

export function normalizeNostrPubkey(value?: string | null) {
  const normalized = value?.trim().toLowerCase() ?? "";
  if (!normalized) {
    return "";
  }

  if (/^[a-f0-9]{64}$/.test(normalized)) {
    return normalized;
  }

  const decoded = bech32Decode(normalized);
  if (!decoded || decoded.prefix !== "npub") {
    return "";
  }

  const bytes = convertBits(decoded.data, 5, 8, false);
  if (!bytes || bytes.length !== 32) {
    return "";
  }

  return bytes.map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

export function resolveMessageAuthorLabel(
  session: MessageAuthorSession,
  message: Pick<MessageItem, "author" | "authorName">,
) {
  switch (message.author) {
    case "me":
      return "You";
    case "peer":
      return normalizedLabel(message.authorName) || (session?.kind === "direct" ? session.name : "Peer");
    default:
      return "System";
  }
}

export function resolveMessageAuthorInitials(
  session: MessageAuthorSession,
  message: Pick<MessageItem, "author" | "authorInitials">,
) {
  switch (message.author) {
    case "me":
      return normalizedLabel(message.authorInitials) || "YO";
    case "peer":
      return normalizedLabel(message.authorInitials) || session?.initials || "PE";
    default:
      return "SY";
  }
}

export function resolveReplyPreviewAuthorLabel(
  session: MessageAuthorSession,
  replyTo: Pick<MessageReplyPreview, "author" | "authorLabel">,
) {
  switch (replyTo.author) {
    case "peer": {
      const label = normalizedLabel(replyTo.authorLabel);
      return label && label !== "Peer" ? label : session?.kind === "direct" ? session.name : "Peer";
    }
    case "me":
      return "You";
    default:
      return normalizedLabel(replyTo.authorLabel) || "System";
  }
}
