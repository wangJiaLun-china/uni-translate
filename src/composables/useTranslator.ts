import { Channel } from "@tauri-apps/api/core";
import { computed, ref } from "vue";
import * as api from "../api/commands";
import type { StreamEvent } from "../types";

export type TranslateStatus = "idle" | "loading" | "streaming" | "done" | "error";

/** Drives one streaming translation at a time. */
export function useTranslator() {
  const status = ref<TranslateStatus>("idle");
  const result = ref("");
  const error = ref("");
  const detectedLang = ref<string | null>(null);
  const targetLang = ref<string | null>(null);
  const busy = computed(() => status.value === "loading" || status.value === "streaming");

  let requestId: string | null = null;
  // Bumped on every start/stop so events from older requests are ignored.
  let generation = 0;

  async function translate(text: string, target: string | null = null) {
    const gen = ++generation;
    result.value = "";
    error.value = "";
    detectedLang.value = null;
    status.value = "loading";

    const channel = new Channel<StreamEvent>();
    channel.onmessage = (event) => {
      if (gen !== generation) return;
      switch (event.type) {
        case "started":
          detectedLang.value = event.detectedLang;
          targetLang.value = event.targetLang;
          break;
        case "delta":
          result.value += event.text;
          status.value = "streaming";
          break;
        case "done":
          result.value = event.fullText.trim();
          status.value = "done";
          break;
        case "error":
          error.value = event.message;
          status.value = "error";
          break;
      }
    };

    try {
      const id = await api.translate(text, target, channel);
      if (gen === generation) requestId = id;
      else void api.cancelTranslation(id); // stopped before the id arrived
    } catch (err) {
      if (gen !== generation) return;
      error.value = api.errorMessage(err);
      status.value = "error";
    }
  }

  function stop() {
    generation++;
    if (requestId) void api.cancelTranslation(requestId);
    requestId = null;
    if (busy.value) status.value = result.value ? "done" : "idle";
  }

  function reset() {
    stop();
    status.value = "idle";
    result.value = "";
    error.value = "";
    detectedLang.value = null;
  }

  return { status, result, error, detectedLang, targetLang, busy, translate, stop, reset };
}
