import type { MessageItem } from "../../types/chat";

interface FileMessageMetaPayloadV1 {
  version: 1;
  label: string;
  localPath: string;
  remoteUrl?: string;
}

interface FileMessageMetaPayloadV2 {
  version: 2;
  label: string;
  localPath?: string;
  remoteUrl?: string;
}

export type FileMessageMetaPayload = FileMessageMetaPayloadV1 | FileMessageMetaPayloadV2;

export function encodeFileMessageMeta(
  payload: Omit<FileMessageMetaPayloadV2, "version">,
): string {
  if (payload.remoteUrl || !payload.localPath) {
    return JSON.stringify({
      version: 2,
      label: payload.label,
      localPath: payload.localPath,
      remoteUrl: payload.remoteUrl,
    } satisfies FileMessageMetaPayloadV2);
  }

  return JSON.stringify({
    version: 1,
    label: payload.label,
    localPath: payload.localPath,
    remoteUrl: payload.remoteUrl,
  } satisfies FileMessageMetaPayloadV1);
}

export function decodeFileMessageMeta(value: string | undefined): FileMessageMetaPayload | null {
  if (!value) {
    return null;
  }

  try {
    const parsed = JSON.parse(value) as Record<string, unknown>;
    if (parsed.version === 2) {
      if (typeof parsed.label !== "string" || !parsed.label.trim()) {
        return null;
      }

      const localPath =
        typeof parsed.localPath === "string" && parsed.localPath.trim()
          ? parsed.localPath.trim()
          : undefined;
      const remoteUrl =
        typeof parsed.remoteUrl === "string" && parsed.remoteUrl.trim()
          ? parsed.remoteUrl.trim()
          : undefined;
      if (!localPath && !remoteUrl) {
        return null;
      }

      return {
        version: 2,
        label: parsed.label.trim(),
        localPath,
        remoteUrl,
      };
    }

    if (
      parsed.version !== 1 ||
      typeof parsed.label !== "string" ||
      !parsed.label.trim()
    ) {
      return null;
    }

    const localPath =
      typeof parsed.localPath === "string" && parsed.localPath.trim()
        ? parsed.localPath.trim()
        : undefined;
    const remoteUrl =
      typeof parsed.remoteUrl === "string" && parsed.remoteUrl.trim()
        ? parsed.remoteUrl.trim()
        : undefined;
    if (!localPath && !remoteUrl) {
      return null;
    }

    return {
      version: 1,
      label: parsed.label.trim(),
      localPath: localPath ?? "",
      remoteUrl,
    };
  } catch {
    return null;
  }
}

export function fileMessageMetaLabel(message: Pick<MessageItem, "kind" | "meta">): string {
  if (message.kind !== "file") {
    return message.meta ?? "";
  }

  return decodeFileMessageMeta(message.meta)?.label ?? message.meta ?? "";
}

export function fileMessageLocalPath(message: Pick<MessageItem, "kind" | "meta">): string {
  if (message.kind !== "file") {
    return "";
  }

  return decodeFileMessageMeta(message.meta)?.localPath ?? "";
}

export function fileMessageRemoteUrl(message: Pick<MessageItem, "kind" | "meta">): string {
  if (message.kind !== "file") {
    return "";
  }

  return decodeFileMessageMeta(message.meta)?.remoteUrl ?? "";
}
