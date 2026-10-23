<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { copyText, openManage } from "../api/commands";
import { useAutoHeight } from "../composables/useAutoHeight";
import { toast } from "../composables/useToast";
import { useTranslator } from "../composables/useTranslator";
import { useSettingsStore } from "../stores/settings";
import Icon from "./Icon.vue";
import SpeakButton from "./SpeakButton.vue";

/**
 * The everyday translation card: the source text on top with a play button,
 * the meaning below in the configured target language.
 */
const props = defineProps<{ mode: "main" | "popup" }>();
const emit = defineEmits<{ close: [] }>();

const store = useSettingsStore();
const { result, error, status, busy, detectedLang, targetLang, translate, stop, reset } = useTranslator();
const source = ref("");
const card = ref<HTMLElement | null>(null);
const input = ref<HTMLTextAreaElement | null>(null);
/** No API configured yet: a setup step, not an error. */
const needsSetup = ref(false);

useAutoHeight(card);

const hasMeaning = computed(
  () => needsSetup.value || !!result.value || !!error.value || status.value === "loading",
);

const direction = computed(() => {
  const target = store.langName(targetLang.value ?? store.settings?.targetLang);
  return detectedLang.value ? `${store.langName(detectedLang.value)} → ${target}` : `译为${target}`;
});

function run() {
  if (!source.value.trim()) return;
  needsSetup.value = !store.configured;
  if (needsSetup.value) {
    reset();
    return;
  }
  void translate(source.value);
}

function onKeydown(event: KeyboardEvent) {
  // Enter translates; Shift+Enter starts a new line. Leave Enter alone while
  // an input method is composing.
  if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
    event.preventDefault();
    run();
  }
}

/** Grows the source box with its text, up to its CSS max-height. */
function fitInput() {
  const el = input.value;
  if (!el) return;
  el.style.height = "auto";
  el.style.height = `${el.scrollHeight}px`;
}

watch(source, () => nextTick(fitInput));

async function copy() {
  await copyText(result.value);
  toast("已复制译文");
}

function openSettings() {
  void openManage("/settings").catch((err) => toast(String(err), "error"));
}

defineExpose({
  /** Shows `text` and translates it, or focuses the empty box. */
  async setSource(text: string) {
    source.value = text;
    needsSetup.value = false;
    if (text.trim()) {
      run();
    } else {
      reset();
      await nextTick();
      input.value?.focus();
    }
  },
  focus: () => input.value?.focus(),
  stop,
});
</script>

<template>
  <!-- Drag the borderless window by any blank part of the card. -->
  <article ref="card" class="card" :class="props.mode" data-tauri-drag-region>
    <div class="source" data-tauri-drag-region>
      <SpeakButton class="pronounce" :text="source" :id="`${props.mode}-source`" />
      <textarea
        ref="input"
        v-model="source"
        rows="1"
        spellcheck="false"
        aria-label="原文"
        :placeholder="props.mode === 'main' ? '输入要翻译的文字，回车翻译' : '没有取到选中的文字，可以在这里输入'"
        @keydown="onKeydown"
      />
      <div class="corner">
        <slot name="corner" />
        <button class="icon-btn close" title="关闭（Esc）" @click="emit('close')">
          <Icon name="close" :size="14" />
        </button>
      </div>
    </div>

    <section v-if="hasMeaning" class="meaning" aria-live="polite" data-tauri-drag-region>
      <template v-if="needsSetup">
        <p class="pending">填好翻译接口的地址和模型后就能翻译了。</p>
        <button class="btn primary" @click="openSettings">打开设置</button>
      </template>
      <p v-else-if="error" class="problem">{{ error }}</p>
      <p v-else-if="result" class="text">{{ result }}<span v-if="busy" class="caret" /></p>
      <p v-else class="pending">正在翻译…</p>
    </section>

    <footer class="meta" data-tauri-drag-region>
      <span class="direction" data-tauri-drag-region>{{ direction }}</span>
      <span class="spacer" data-tauri-drag-region />
      <button v-if="busy" class="icon-btn" title="停止" @click="stop"><Icon name="stop" :size="14" /></button>
      <button v-if="result && !busy" class="icon-btn" title="复制译文" @click="copy">
        <Icon name="copy" :size="14" />
      </button>
      <slot name="actions" />
    </footer>
  </article>
</template>

<style scoped>
.card {
  display: flex;
  flex-direction: column;
  /* The window follows this height; past it the meaning scrolls. */
  max-height: 560px;
  background: var(--paper);
  border: 1px solid var(--rule);
  /* The window is transparent, so these corners are the window's corners. */
  border-radius: 12px;
  overflow: hidden;
}

.source {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 8px 6px 6px 8px;
}
/* A pronunciation mark, like in a dictionary entry. */
.pronounce {
  width: 24px;
  height: 24px;
  border-radius: 50%;
  color: var(--indigo);
  background: var(--indigo-wash);
  flex-shrink: 0;
}
.pronounce:hover:not(:disabled) {
  background: var(--indigo-wash);
  color: var(--indigo);
  filter: brightness(0.96);
}
.source textarea {
  flex: 1;
  min-width: 0;
  max-height: 7.5em;
  padding: 2px 0;
  border: none;
  outline: none;
  resize: none;
  background: transparent;
  color: var(--ink-soft);
  font: inherit;
  font-size: 13px;
  line-height: 1.55;
}
.source textarea:focus {
  color: var(--ink);
}
.source textarea::placeholder {
  color: var(--ink-soft);
  opacity: 0.7;
}
.corner {
  display: flex;
  flex-shrink: 0;
}
.corner :deep(.icon-btn) {
  width: 22px;
  height: 22px;
  opacity: 0.55;
}
.corner :deep(.icon-btn:hover),
.corner :deep(.icon-btn.active),
.corner :deep(.icon-btn:focus-visible) {
  opacity: 1;
}

.meaning {
  min-height: 0;
  overflow: auto;
  margin: 0 12px;
  padding: 8px 0 10px;
  border-top: 1px solid var(--rule);
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 8px;
}
.meaning p {
  margin: 0;
}
.text {
  font-family: var(--serif);
  font-size: 16px;
  line-height: 1.6;
  color: var(--ink);
  white-space: pre-wrap;
  word-break: break-word;
  user-select: text;
}
.pending {
  color: var(--ink-soft);
  font-size: 13px;
}
.problem {
  color: var(--danger);
  font-size: 13px;
}

.meta {
  display: flex;
  align-items: center;
  gap: 2px;
  min-height: 28px;
  padding: 0 6px 2px 12px;
  font-size: 12px;
  color: var(--ink-soft);
}
.meta :deep(.icon-btn) {
  width: 24px;
  height: 24px;
}
.direction {
  white-space: nowrap;
}
</style>
