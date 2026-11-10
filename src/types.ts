// Mirrors the serde types in src-tauri/src.

export type TtsProvider = "openai" | "baidu";

export interface TtsSettings {
  provider: TtsProvider;
  useSameEndpoint: boolean;
  baseUrl: string;
  model: string;
  voice: string;
  speed: number;
  /** Baidu voice id (`per`). */
  baiduVoice: number;
}

export interface Settings {
  baseUrl: string;
  model: string;
  temperature: number;
  /** Ask reasoning models to answer without thinking first. */
  disableThinking: boolean;
  /** Model that reads screenshots; empty means `model`. */
  visionModel: string;
  targetLang: string;
  secondaryLang: string;
  tts: TtsSettings;
  hotkey: string;
  screenshotHotkey: string;
  historyEnabled: boolean;
}

export interface Language {
  code: string;
  name: string;
  english: string;
}

export type KeyKind = "chat" | "tts" | "baidu_api_key" | "baidu_secret_key";

export type StreamEvent =
  | { type: "started"; detectedLang: string | null; targetLang: string }
  | { type: "delta"; text: string }
  | { type: "done"; fullText: string }
  | { type: "error"; message: string };

export interface HistoryItem {
  id: number;
  createdAt: number;
  sourceText: string;
  resultText: string;
  sourceLang: string | null;
  targetLang: string;
  model: string;
}
