<script setup lang="ts">
import { computed } from "vue";
import { speakText, speaker, stopSpeaking } from "../composables/useSpeaker";
import Icon from "./Icon.vue";

const props = defineProps<{ text: string; id: string }>();

const active = computed(() => speaker.activeId === props.id);

function toggle() {
  if (active.value) stopSpeaking();
  else void speakText(props.text, props.id);
}
</script>

<template>
  <button
    class="icon-btn"
    :class="{ active }"
    :disabled="!text.trim() && !active"
    :title="active ? '停止朗读' : '朗读'"
    @click="toggle"
  >
    <Icon :name="active ? (speaker.loading ? 'loader' : 'stop') : 'speaker'" />
  </button>
</template>
