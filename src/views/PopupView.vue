<script setup lang="ts">
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { copyText, takePendingSelection } from "../api/commands";
import Icon from "../components/Icon.vue";
import SpeakButton from "../components/SpeakButton.vue";
import { stopSpeaking } from "../composables/useSpeaker";
import { toast } from "../composables/useToast";
import { useTranslator } from "../composables/useTranslator";
import { useSettingsStore } from "../stores/settings";

const store = useSettingsStore();
const { result, error, status, busy, detectedLang, targetLang, translate, stop, reset } = useTranslator();
const win = getCurrentWebviewWindow();
const source = ref("");
const pinned = ref(false);
const sourceInput = ref<HTMLTextAreaElement | null>(null);
const unlisten: UnlistenFn[] = [];

/** Picks up text grabbed by the hotkey. */
async function loadSelection() {
  const text = await takePendingSelection();
  if (text === null) return;
  await store.load(); // settings may have changed in the main window
  source.value = text;
  if (text.trim()) {
    run();
  } else {
    reset();
    await nextTick();
    sourceInput.value?.focus();
  }
}

function run() {
  if (!source.value.trim()) return;
  if (!store.configured) {
    reset();
    error.value = "还没有配置翻译接口，请在主窗口的设置中填写";
    status.value = "error";
    return;
  }
  void translate(source.value);
}

function hide() {
  stop();
  stopSpeaking();
  void win.hide();
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") hide();
}

function onSourceKeydown(event: KeyboardEvent) {
  if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
    event.preventDefault();
    run();
  }
}

async function copy() {
  await copyText(result.value);
  toast("已复制");
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
  <div class="popup">
    <header class="popup-header" data-tauri-drag-region>
      <span class="pill" data-tauri-drag-region>
        {{ detectedLang ? store.langName(detectedLang) : "自动检测" }}
        <Icon name="arrow" :size="12" />
        {{ store.langName(targetLang ?? store.settings?.targetLang) }}
      </span>
      <span class="spacer" data-tauri-drag-region />
      <button class="icon-btn" :class="{ active: pinned }" :title="pinned ? '取消固定' : '固定窗口'" @click="pinned = !pinned">
        <Icon name="pin" />
      </button>
      <button class="icon-btn" title="关闭 (Esc)" @click="hide">
        <Icon name="close" />
      </button>
    </header>

    <textarea
      ref="sourceInput"
      v-model="source"
      class="source"
      rows="2"
      placeholder="没有获取到选中的文字，可以在这里输入，Ctrl / ⌘ + Enter 翻译"
      @keydown="onSourceKeydown"
    />

    <div class="result">
      <p v-if="error" class="error-text">{{ error }}</p>
      <template v-else-if="result">
        <span class="result-text">{{ result }}</span>
        <span v-if="busy" class="caret" />
      </template>
      <span v-else-if="status === 'loading'" class="muted"><Icon name="loader" /> 翻译中…</span>
    </div>

    <footer class="popup-footer">
      <span class="spacer" />
      <button class="icon-btn" title="重新翻译" :disabled="!source.trim() || busy" @click="run">
        <Icon name="refresh" />
      </button>
      <SpeakButton :text="busy ? '' : result" id="popup-result" />
      <button class="icon-btn" title="复制" :disabled="!result" @click="copy">
        <Icon name="copy" />
      </button>
    </footer>
  </div>
</template>

<style scoped>
.popup {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--surface);
  border: 1px solid var(--border);
  overflow: hidden;
}
.popup-header,
.popup-footer {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 6px 8px;
}
.popup-header {
  border-bottom: 1px solid var(--border);
  cursor: move;
}
.source {
  padding: 8px 12px;
  border: none;
  border-bottom: 1px dashed var(--border);
  outline: none;
  resize: none;
  background: transparent;
  color: var(--muted);
  font: inherit;
  font-size: 13px;
  line-height: 1.5;
  max-height: 30%;
}
.source:focus {
  color: var(--text);
}
.result {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: 10px 12px;
  font-size: 15px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-word;
  user-select: text;
}
.popup-footer {
  border-top: 1px solid var(--border);
}
</style>
