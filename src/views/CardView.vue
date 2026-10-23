<script setup lang="ts">
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { onBeforeUnmount, onMounted, ref } from "vue";
import { openManage, type ManageRoute } from "../api/commands";
import Icon from "../components/Icon.vue";
import TranslateCard from "../components/TranslateCard.vue";
import { stopSpeaking } from "../composables/useSpeaker";
import { toast } from "../composables/useToast";
import { useSettingsStore } from "../stores/settings";

const store = useSettingsStore();
const win = getCurrentWebviewWindow();
const card = ref<InstanceType<typeof TranslateCard> | null>(null);
let unlisten: (() => void) | undefined;

/** Hides the card; the tray icon brings it back. */
function hide() {
  card.value?.stop();
  stopSpeaking();
  void win.hide();
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") hide();
}

function open(route: ManageRoute) {
  void openManage(route).catch((err) => toast(String(err), "error"));
}

onMounted(async () => {
  card.value?.focus();
  window.addEventListener("keydown", onKeydown);
  // Settings may have changed in the settings window meanwhile.
  unlisten = await win.onFocusChanged(({ payload: focused }) => {
    if (focused) void store.load().catch(() => {});
  });
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
  unlisten?.();
});
</script>

<template>
  <TranslateCard ref="card" mode="main" @close="hide">
    <template #actions>
      <button class="icon-btn" title="历史记录" @click="open('/history')"><Icon name="history" :size="14" /></button>
      <button class="icon-btn" title="设置" @click="open('/settings')"><Icon name="settings" :size="14" /></button>
    </template>
  </TranslateCard>
</template>
