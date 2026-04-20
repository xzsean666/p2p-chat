import { resolveLocalMediaAssetUrl } from "./localMediaAsset";
import type { MessageItem } from "../../types/chat";

interface ImageMessageMetaPayloadV1 {
  version: 1;
  label: string;
  previewDataUrl: string;
}

interface ImageMessageMetaPayloadV2 {
  version: 2;
  label: string;
  localPath: string;
}

interface ImageMessageMetaPayloadV3 {
  version: 3;
  label: string;
  localPath?: string;
  remoteUrl?: string;
}

export type ImageMessageMetaPayload =
  | ImageMessageMetaPayloadV1
  | ImageMessageMetaPayloadV2
  | ImageMessageMetaPayloadV3;

export function encodeImageMessageMeta(
  payload: Omit<ImageMessageMetaPayloadV2, "version">,
): string {
  return JSON.stringify({
    version: 2,
    label: payload.label,
    localPath: payload.localPath,
  } satisfies ImageMessageMetaPayloadV2);
}

export function decodeImageMessageMeta(value: string | undefined): ImageMessageMetaPayload | null {
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

export function imageMessageMetaLabel(message: Pick<MessageItem, "kind" | "meta">): string {
  if (message.kind !== "image") {
    return message.meta ?? "";
  }

  return decodeImageMessageMeta(message.meta)?.label ?? "";
}

export function imageMessagePreviewUrl(message: Pick<MessageItem, "kind" | "meta">): string {
  if (message.kind !== "image") {
    return "";
  }

  const payload = decodeImageMessageMeta(message.meta);
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

export function imageMessageLocalPath(message: Pick<MessageItem, "kind" | "meta">): string {
  if (message.kind !== "image") {
    return "";
  }

  const payload = decodeImageMessageMeta(message.meta);
  return payload?.version === 2 || payload?.version === 3 ? payload.localPath ?? "" : "";
}

export function imageMessageRemoteUrl(message: Pick<MessageItem, "kind" | "meta">): string {
  if (message.kind !== "image") {
    return "";
  }

  const payload = decodeImageMessageMeta(message.meta);
  return payload?.version === 3 ? payload.remoteUrl ?? "" : "";
}
