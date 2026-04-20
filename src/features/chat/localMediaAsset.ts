import { convertFileSrc } from "@tauri-apps/api/core";

function hasTauriRuntime() {
  const globalWindow = globalThis as typeof globalThis & {
    __TAURI__?: unknown;
    __TAURI_INTERNALS__?: unknown;
  };

  return typeof window !== "undefined" && ("__TAURI_INTERNALS__" in globalWindow || "__TAURI__" in globalWindow);
}

export function resolveLocalMediaAssetUrl(source: string | undefined): string {
  const normalized = source?.trim() ?? "";
  if (!normalized) {
    return "";
  }

  if (
    normalized.startsWith("data:") ||
    normalized.startsWith("asset:") ||
    normalized.startsWith("http://asset.localhost") ||
    normalized.startsWith("https://") ||
    normalized.startsWith("http://")
  ) {
    return normalized;
  }

  if (!hasTauriRuntime()) {
    return normalized;
  }

  try {
    return convertFileSrc(normalized);
  } catch {
    return normalized;
  }
}
