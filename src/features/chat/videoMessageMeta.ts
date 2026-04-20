import { resolveLocalMediaAssetUrl } from "./localMediaAsset";
import type { MessageItem } from "../../types/chat";

interface VideoMessageMetaPayloadV1 {
  version: 1;
  label: string;
  previewDataUrl: string;
}

interface VideoMessageMetaPayloadV2 {
  version: 2;
  label: string;
  localPath: string;
}

interface VideoMessageMetaPayloadV3 {
  version: 3;
  label: string;
  localPath?: string;
  remoteUrl?: string;
}

export type VideoMessageMetaPayload =
  | VideoMessageMetaPayloadV1
  | VideoMessageMetaPayloadV2
  | VideoMessageMetaPayloadV3;

export function encodeVideoMessageMeta(
  payload: Omit<VideoMessageMetaPayloadV2, "version">,
): string {
  return JSON.stringify({
    version: 2,
    label: payload.label,
    localPath: payload.localPath,
  } satisfies VideoMessageMetaPayloadV2);
}

export function decodeVideoMessageMeta(value: string | undefined): VideoMessageMetaPayload | null {
  if (!value) {
    return null;
  }

  try {
    const parsed = JSON.parse(value) as Record<string, unknown>;
    if (parsed.version === 1) {
      if (
        typeof parsed.label !== "string" ||
        !parsed.label.trim() ||
        typeof parsed.previewDataUrl !== "string" ||
        !parsed.previewDataUrl.trim()
      ) {
        return null;
      }

      return {
        version: 1,
        label: parsed.label.trim(),
        previewDataUrl: parsed.previewDataUrl.trim(),
      };
    }

    if (parsed.version === 3) {
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
        version: 3,
        label: parsed.label.trim(),
        localPath,
        remoteUrl,
      };
    }

    if (
      parsed.version !== 2 ||
      typeof parsed.label !== "string" ||
      !parsed.label.trim() ||
      typeof parsed.localPath !== "string" ||
      !parsed.localPath.trim()
    ) {
      return null;
    }

    return {
      version: 2,
      label: parsed.label.trim(),
      localPath: parsed.localPath.trim(),
    };
  } catch {
    return null;
  }
}

export function videoMessageMetaLabel(message: Pick<MessageItem, "kind" | "meta">): string {
  if (message.kind !== "video") {
    return message.meta ?? "";
  }

  return decodeVideoMessageMeta(message.meta)?.label ?? "";
}

export function videoMessagePreviewUrl(message: Pick<MessageItem, "kind" | "meta">): string {
  if (message.kind !== "video") {
    return "";
  }

  const payload = decodeVideoMessageMeta(message.meta);
  if (!payload) {
    return "";
  }

  if (payload.version === 1) {
    return payload.previewDataUrl;
  }

  if (payload.version === 2) {
    return resolveLocalMediaAssetUrl(payload.localPath);
  }

  return resolveLocalMediaAssetUrl(payload.localPath ?? payload.remoteUrl);
}

export function videoMessageLocalPath(message: Pick<MessageItem, "kind" | "meta">): string {
  if (message.kind !== "video") {
    return "";
  }

  const payload = decodeVideoMessageMeta(message.meta);
  return payload?.version === 2 || payload?.version === 3 ? payload.localPath ?? "" : "";
}

export function videoMessageRemoteUrl(message: Pick<MessageItem, "kind" | "meta">): string {
  if (message.kind !== "video") {
    return "";
  }

  const payload = decodeVideoMessageMeta(message.meta);
  return payload?.version === 3 ? payload.remoteUrl ?? "" : "";
}
