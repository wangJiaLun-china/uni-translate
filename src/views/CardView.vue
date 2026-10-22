<script setup lang="ts">
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { onBeforeUnmount, onMounted, ref } from "vue";
import { openManage, type ManageRoute } from "../api/commands";
import Icon from "../components/Icon.vue";
import TranslateCard from "../components/TranslateCard.vue";
import { toast } from "../composables/useToast";
import { useSettingsStore } from "../stores/settings";

const store = useSettingsStore();
const card = ref<InstanceType<typeof TranslateCard> | null>(null);
let unlisten: (() => void) | undefined;

function open(route: ManageRoute) {
  void openManage(route).catch((err) => toast(String(err), "error"));
}

onMounted(async () => {
  card.value?.focus();
  // Settings may have changed in the settings window meanwhile.
  unlisten = await getCurrentWebviewWindow().onFocusChanged(({ payload: focused }) => {
    if (focused) void store.load().catch(() => {});
  });
});

onBeforeUnmount(() => unlisten?.());
</script>

<template>
  <TranslateCard ref="card" mode="main">
    <template #actions>
      <button class="icon-btn" title="历史记录" @click="open('/history')"><Icon name="history" /></button>
      <button class="icon-btn" title="设置" @click="open('/settings')"><Icon name="settings" /></button>
    </template>
  </TranslateCard>
</template>
