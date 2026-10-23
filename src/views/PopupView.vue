<script setup lang="ts">
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { onBeforeUnmount, onMounted, ref } from "vue";
import { takePendingSelection } from "../api/commands";
import Icon from "../components/Icon.vue";
import TranslateCard from "../components/TranslateCard.vue";
import { stopSpeaking } from "../composables/useSpeaker";
import { useSettingsStore } from "../stores/settings";

const store = useSettingsStore();
const win = getCurrentWebviewWindow();
const card = ref<InstanceType<typeof TranslateCard> | null>(null);
const pinned = ref(false);
const unlisten: UnlistenFn[] = [];

/** Picks up text grabbed by the hotkey. */
async function loadSelection() {
  const text = await takePendingSelection();
  if (text === null) return;
  await store.load(); // settings may have changed in the settings window
  await card.value?.setSource(text);
}

function hide() {
  card.value?.stop();
  stopSpeaking();
  void win.hide();
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") hide();
}

onMounted(async () => {
  window.addEventListener("keydown", onKeydown);
  unlisten.push(await listen("popup://selection", loadSelection));
  unlisten.push(
    await win.onFocusChanged(({ payload: focused }) => {
      if (!focused && !pinned.value) hide();
    }),
  );
  await loadSelection();
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
  unlisten.forEach((fn) => fn());
});
</script>

<template>
  <TranslateCard ref="card" mode="popup" @close="hide">
    <template #corner>
      <button
        class="icon-btn"
        :class="{ active: pinned }"
        :title="pinned ? '取消固定' : '固定，失去焦点时不关闭'"
        :aria-pressed="pinned"
        @click="pinned = !pinned"
      >
        <Icon name="pin" :size="14" />
      </button>
    </template>
  </TranslateCard>
</template>
