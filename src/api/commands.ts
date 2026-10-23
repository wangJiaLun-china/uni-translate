import { Channel, invoke } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { HistoryItem, KeyKind, Language, Settings, StreamEvent } from "../types";

export const getSettings = () => invoke<Settings>("get_settings");

export const saveSettings = (settings: Settings) => invoke<Settings>("save_settings", { settings });

/** An empty key deletes the stored one. */
export const setApiKey = (kind: KeyKind, key: string) => invoke<void>("set_api_key", { kind, key });

export const hasApiKey = (kind: KeyKind) => invoke<boolean>("has_api_key", { kind });

export const listLanguages = () => invoke<Language[]>("list_languages");

/** Starts a streaming translation; resolves to the request id. */
export const translate = (text: string, targetLang: string | null, onEvent: Channel<StreamEvent>) =>
  invoke<string>("translate", { text, targetLang, onEvent });

export const cancelTranslation = (id: string) => invoke<void>("cancel_translation", { id });

/** An empty `apiKey` means "use the saved key". */
export const testConnection = (baseUrl: string, model: string, apiKey: string) =>
  invoke<string>("test_connection", { baseUrl, model, apiKey });

export const listModels = (baseUrl: string, apiKey: string) =>
  invoke<string[]>("list_models", { baseUrl, apiKey });

/** Returns MP3 audio. */
export const speak = (text: string) => invoke<ArrayBuffer>("speak", { text });

export const splitTtsText = (text: string) => invoke<string[]>("split_tts_text", { text });

export const listHistory = (query: string, limit: number, offset: number) =>
  invoke<HistoryItem[]>("list_history", { query, limit, offset });

export const deleteHistory = (id: number) => invoke<void>("delete_history", { id });

export const clearHistory = () => invoke<void>("clear_history");

export const takePendingSelection = () => invoke<string | null>("take_pending_selection");

export const copyText = (text: string) => writeText(text);

/** Baidu AI Cloud's speech console, where users get their API keys. */
export const BAIDU_CONSOLE_URL = "https://console.bce.baidu.com/ai-engine/speech/overview/";

/** Opens the console in the system browser (only this site is allowed). */
export const openBaiduConsole = () => openUrl(BAIDU_CONSOLE_URL);

/** Commands reject with the error's message string. */
export function errorMessage(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  return String(err);
}

export type ManageRoute = "/settings" | "/history";

/** Opens the settings/history window, creating it if needed. */
export const openManage = (route: ManageRoute) => invoke<void>("open_manage", { route });

export const takeManageRoute = () => invoke<string | null>("take_manage_route");
