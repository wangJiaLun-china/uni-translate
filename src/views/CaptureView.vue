<script setup lang="ts">
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { cancelCapture, errorMessage, finishCapture, screenImage } from "../api/commands";

/**
 * Region picker for screenshot translation on Windows and Linux (macOS uses
 * the system's own). The screenshot fills the window, dimmed; the dragged-out
 * region shows at full brightness.
 */
const win = getCurrentWebviewWindow();
const imageUrl = ref<string | null>(null);
/** The new screenshot has loaded; until then the transparent window shows nothing. */
const ready = ref(false);
const start = ref<{ x: number; y: number } | null>(null);
const end = ref<{ x: number; y: number } | null>(null);
let unlisten: UnlistenFn | undefined;

const rect = computed(() => {
  if (!start.value || !end.value) return null;
  const x = Math.min(start.value.x, end.value.x);
  const y = Math.min(start.value.y, end.value.y);
  return { x, y, width: Math.abs(end.value.x - start.value.x), height: Math.abs(end.value.y - start.value.y) };
});

/** Loads the new screenshot, then shows the window over the screen. */
async function load() {
  start.value = end.value = null;
  ready.value = false;
  try {
    const jpeg = await screenImage();
    if (imageUrl.value) URL.revokeObjectURL(imageUrl.value);
    imageUrl.value = URL.createObjectURL(new Blob([jpeg], { type: "image/jpeg" }));
  } catch (err) {
    // No screenshot pending (e.g. the page reloaded); stay hidden.
    console.warn(errorMessage(err));
  }
}

async function onImageLoaded() {
  ready.value = true;
  await win.setFocus();
}

async function close(selection?: NonNullable<typeof rect.value>) {
  start.value = end.value = null;
  ready.value = false;
  await win.hide();
  if (selection) {
    await finishCapture({ ...selection, viewWidth: window.innerWidth, viewHeight: window.innerHeight });
  } else {
    await cancelCapture();
  }
}

function onMouseDown(event: MouseEvent) {
  if (event.button === 2) {
    void close();
    return;
  }
  if (event.button !== 0) return;
  start.value = end.value = { x: event.clientX, y: event.clientY };
}

function onMouseMove(event: MouseEvent) {
  if (start.value) end.value = { x: event.clientX, y: event.clientY };
}

function onMouseUp(event: MouseEvent) {
  if (event.button !== 0 || !rect.value) return;
  void close(rect.value);
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") void close();
}

onMounted(async () => {
  window.addEventListener("keydown", onKeydown);
  unlisten = await listen("capture://start", load);
  await load();
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
  unlisten?.();
});
</script>

<template>
  <div
    class="picker"
    @mousedown="onMouseDown"
    @mousemove="onMouseMove"
    @mouseup="onMouseUp"
    @contextmenu.prevent
  >
    <img v-if="imageUrl" v-show="ready" class="shot" :src="imageUrl" alt="" draggable="false" @load="onImageLoaded" />
    <!-- Before a drag the whole screen is dimmed; during one, the region's
         shadow dims everything around it. -->
    <div v-if="ready && !rect" class="dim" />
    <div
      v-if="rect"
      class="region"
      :style="{ left: `${rect.x}px`, top: `${rect.y}px`, width: `${rect.width}px`, height: `${rect.height}px` }"
    >
      <span class="size">{{ Math.round(rect.width) }} × {{ Math.round(rect.height) }}</span>
    </div>
    <p v-else-if="ready" class="hint">拖动框选要翻译的区域，按 Esc 取消</p>
  </div>
</template>

<style scoped>
.picker {
  position: fixed;
  inset: 0;
  cursor: crosshair;
  user-select: none;
  overflow: hidden;
}
.shot {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
}
.dim {
  position: absolute;
  inset: 0;
  background: rgb(0 0 0 / 0.4);
}
.region {
  position: absolute;
  outline: 1px solid #fff;
  box-shadow: 0 0 0 100vmax rgb(0 0 0 / 0.4);
}
.size {
  position: absolute;
  left: 0;
  top: -22px;
  padding: 1px 6px;
  border-radius: 4px;
  background: var(--indigo);
  color: var(--on-indigo);
  font-size: 12px;
  white-space: nowrap;
}
.hint {
  position: absolute;
  top: 24px;
  left: 50%;
  transform: translateX(-50%);
  margin: 0;
  padding: 6px 12px;
  border-radius: 6px;
  background: rgb(0 0 0 / 0.6);
  color: #fff;
  font-size: 13px;
  pointer-events: none;
}
</style>
