import { defineStore } from "pinia";
import { computed, ref } from "vue";
import * as api from "../api/commands";
import type { Language, Settings } from "../types";

export const useSettingsStore = defineStore("settings", () => {
  const settings = ref<Settings | null>(null);
  const languages = ref<Language[]>([]);

  const configured = computed(() => !!settings.value?.baseUrl && !!settings.value?.model);

  async function load() {
    const [loaded, langs] = await Promise.all([api.getSettings(), api.listLanguages()]);
    settings.value = loaded;
    languages.value = langs;
  }

  async function save(next: Settings) {
    settings.value = await api.saveSettings(next);
    return settings.value;
  }

  /** Names a full code (`zh-CN`) or a detected base code (`zh`). */
  function langName(code: string | null | undefined): string {
    if (!code) return "";
    if (code === "zh") return "中文";
    return languages.value.find((l) => l.code === code)?.name ?? code;
  }

  return { settings, languages, configured, load, save, langName };
});
