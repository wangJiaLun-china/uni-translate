<script setup lang="ts">
import { computed, ref } from "vue";
import { copyText } from "../api/commands";
import Icon from "../components/Icon.vue";
import SpeakButton from "../components/SpeakButton.vue";
import { toast } from "../composables/useToast";
import { useTranslator } from "../composables/useTranslator";
import { useSettingsStore } from "../stores/settings";

const store = useSettingsStore();
const translator = useTranslator();
const source = ref("");

const target = computed({
  get: () => store.settings?.targetLang ?? "zh-CN",
  set: (code: string) => {
    if (!store.settings) return;
    void store.save({ ...store.settings, targetLang: code }).catch((err) => toast(String(err), "error"));
  },
});

/** The swap to the second language is worth pointing out. */
const swapped = computed(
  () => translator.targetLang.value !== null && translator.targetLang.value !== target.value,
);

function run() {
  if (!source.value.trim() || !store.configured) return;
  void translator.translate(source.value, target.value);
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
    event.preventDefault();
    run();
  }
}

function clear() {
  source.value = "";
  translator.reset();
}

async function copy() {
  await copyText(translator.result.value);
  toast("已复制");
}
</script>

<template>
  <div class="translate">
    <div v-if="store.settings && !store.configured" class="banner">
      还没有配置翻译接口。
      <RouterLink to="/settings">去设置</RouterLink>
    </div>

    <div class="toolbar">
      <span class="pill" :title="'识别到的原文语言'">
        {{ translator.detectedLang.value ? store.langName(translator.detectedLang.value) : "自动检测" }}
      </span>
      <Icon name="arrow" class="muted" />
      <select v-model="target" class="select" title="目标语言">
        <option v-for="lang in store.languages" :key="lang.code" :value="lang.code">{{ lang.name }}</option>
      </select>
      <span v-if="swapped" class="muted small">
        原文已是目标语言，已改译为{{ store.langName(translator.targetLang.value) }}
      </span>
      <span class="spacer" />
      <button v-if="translator.busy.value" class="btn" @click="translator.stop()">
        <Icon name="stop" /> 停止
      </button>
      <button
        v-else
        class="btn primary"
        :disabled="!source.trim() || !store.configured"
        title="Ctrl / ⌘ + Enter"
        @click="run"
      >
        <Icon name="translate" /> 翻译
      </button>
    </div>

    <div class="panes">
      <section class="pane">
        <textarea
          v-model="source"
          class="pane-body"
          placeholder="输入要翻译的文字，Ctrl / ⌘ + Enter 翻译"
          autofocus
          @keydown="onKeydown"
        />
        <footer class="pane-footer">
          <span class="muted small">{{ source.length }} 字</span>
          <span class="spacer" />
          <SpeakButton :text="source" id="main-source" />
          <button class="icon-btn" title="清空" :disabled="!source" @click="clear">
            <Icon name="close" />
          </button>
        </footer>
      </section>

      <section class="pane result">
        <div class="pane-body" :class="{ placeholder: !translator.result.value && !translator.error.value }">
          <template v-if="translator.error.value">
            <p class="error-text">{{ translator.error.value }}</p>
          </template>
          <template v-else-if="translator.result.value">
            <span class="result-text">{{ translator.result.value }}</span>
            <span v-if="translator.busy.value" class="caret" />
          </template>
          <template v-else-if="translator.status.value === 'loading'">
            <span class="muted"><Icon name="loader" /> 翻译中…</span>
          </template>
          <template v-else>译文会显示在这里</template>
        </div>
        <footer class="pane-footer">
          <span class="spacer" />
          <SpeakButton :text="translator.busy.value ? '' : translator.result.value" id="main-result" />
          <button class="icon-btn" title="复制" :disabled="!translator.result.value" @click="copy">
            <Icon name="copy" />
          </button>
        </footer>
      </section>
    </div>
  </div>
</template>

<style scoped>
.translate {
  display: flex;
  flex-direction: column;
  gap: 12px;
  height: 100%;
}
.panes {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
  gap: 12px;
}
.pane {
  display: flex;
  flex-direction: column;
  min-height: 160px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--surface);
  overflow: hidden;
}
.pane:focus-within {
  border-color: var(--accent);
}
.pane-body {
  flex: 1;
  min-height: 0;
  padding: 12px 14px;
  overflow: auto;
  border: none;
  outline: none;
  resize: none;
  background: transparent;
  color: var(--text);
  font: inherit;
  font-size: 15px;
  line-height: 1.6;
}
.result .pane-body {
  white-space: pre-wrap;
  word-break: break-word;
  user-select: text;
}
.pane-body.placeholder {
  color: var(--muted);
}
.pane-footer {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 8px;
  border-top: 1px solid var(--border);
}
</style>
