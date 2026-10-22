<script setup lang="ts">
import { computed, ref } from "vue";

const model = defineModel<string>({ required: true });
const recording = ref(false);
const hint = ref("");

const MODIFIER_KEYS = ["Control", "Shift", "Alt", "Meta"];

/** "Alt+KeyD" -> "Alt + D" */
const display = computed(() =>
  model.value
    .split("+")
    .map((part) => part.replace(/^Key(?=[A-Z]$)/, "").replace(/^Digit(?=\d$)/, ""))
    .join(" + "),
);

function onKeydown(event: KeyboardEvent) {
  event.preventDefault();
  if (event.key === "Escape") {
    (event.target as HTMLElement).blur();
    return;
  }
  if (MODIFIER_KEYS.includes(event.key)) return;

  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Super");
  if (!parts.length && !/^F\d+$/.test(event.code)) {
    hint.value = "请至少包含一个修饰键（Ctrl / Alt / Shift / ⌘）";
    return;
  }
  hint.value = "";
  model.value = [...parts, event.code].join("+");
  (event.target as HTMLElement).blur();
}
</script>

<template>
  <div class="hotkey">
    <input
      class="input"
      readonly
      :value="recording ? '请按下快捷键…' : display"
      @focus="recording = true"
      @blur="recording = false"
      @keydown="onKeydown"
    />
    <span v-if="hint" class="hint error-text">{{ hint }}</span>
  </div>
</template>

<style scoped>
.hotkey input {
  cursor: pointer;
  caret-color: transparent;
}
</style>
