<script setup lang="ts">
import { toasts } from "../composables/useToast";
</script>

<template>
  <div class="toasts" aria-live="polite">
    <div v-for="t in toasts" :key="t.id" class="toast" :class="t.kind">{{ t.message }}</div>
  </div>
</template>

<style scoped>
.toasts {
  position: fixed;
  left: 50%;
  top: 64px;
  transform: translateX(-50%);
  display: flex;
  flex-direction: column;
  gap: 8px;
  z-index: 100;
  pointer-events: none;
  max-width: calc(100vw - 32px);
}
.toast {
  padding: 8px 14px;
  border-radius: 8px;
  background: var(--text);
  color: var(--bg);
  font-size: 13px;
  box-shadow: var(--shadow);
  text-align: center;
}
.toast.error {
  background: var(--danger);
  color: #fff;
}
</style>
