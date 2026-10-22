<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import * as api from "../api/commands";
import Icon from "../components/Icon.vue";
import SpeakButton from "../components/SpeakButton.vue";
import { toast } from "../composables/useToast";
import { useSettingsStore } from "../stores/settings";
import type { HistoryItem } from "../types";

const PAGE_SIZE = 50;

const store = useSettingsStore();
const items = ref<HistoryItem[]>([]);
const query = ref("");
const hasMore = ref(false);
const loading = ref(false);
const confirmingClear = ref(false);
let searchTimer: ReturnType<typeof setTimeout> | undefined;
let clearTimer: ReturnType<typeof setTimeout> | undefined;

async function load(append = false) {
  loading.value = true;
  try {
    const page = await api.listHistory(query.value, PAGE_SIZE, append ? items.value.length : 0);
    items.value = append ? [...items.value, ...page] : page;
    hasMore.value = page.length === PAGE_SIZE;
  } catch (err) {
    toast(api.errorMessage(err), "error");
  } finally {
    loading.value = false;
  }
}

onMounted(() => load());

watch(query, () => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => load(), 300);
});

async function remove(item: HistoryItem) {
  await api.deleteHistory(item.id);
  items.value = items.value.filter((i) => i.id !== item.id);
}

async function clearAll() {
  if (!confirmingClear.value) {
    confirmingClear.value = true;
    clearTimer = setTimeout(() => (confirmingClear.value = false), 3000);
    return;
  }
  clearTimeout(clearTimer);
  confirmingClear.value = false;
  await api.clearHistory();
  items.value = [];
  hasMore.value = false;
  toast("已清空历史记录");
}

async function copy(text: string) {
  await api.copyText(text);
  toast("已复制");
}

const timeFormat = new Intl.DateTimeFormat("zh-CN", { dateStyle: "short", timeStyle: "short" });
</script>

<template>
  <div class="history">
    <div class="toolbar">
      <input v-model="query" class="input search" placeholder="搜索原文或译文" />
      <button class="btn" :class="{ danger: confirmingClear }" :disabled="!items.length && !confirmingClear" @click="clearAll">
        <Icon name="trash" /> {{ confirmingClear ? "确认清空？" : "清空" }}
      </button>
    </div>

    <p v-if="!store.settings?.historyEnabled" class="muted small">历史记录已在设置中关闭，新的翻译不会被保存。</p>

    <ul v-if="items.length" class="list">
      <li v-for="item in items" :key="item.id" class="item">
        <div class="meta muted small">
          <span>{{ timeFormat.format(item.createdAt) }}</span>
          <span>
            {{ item.sourceLang ? store.langName(item.sourceLang) : "自动" }} → {{ store.langName(item.targetLang) }}
          </span>
          <span class="model">{{ item.model }}</span>
          <span class="spacer" />
          <SpeakButton :text="item.resultText" :id="`history-${item.id}`" />
          <button class="icon-btn" title="复制译文" @click="copy(item.resultText)"><Icon name="copy" /></button>
          <button class="icon-btn" title="删除" @click="remove(item)"><Icon name="trash" /></button>
        </div>
        <p class="source">{{ item.sourceText }}</p>
        <p class="result-text">{{ item.resultText }}</p>
      </li>
    </ul>
    <p v-else-if="!loading" class="empty muted">{{ query ? "没有匹配的记录" : "还没有翻译记录" }}</p>

    <button v-if="hasMore" class="btn more" :disabled="loading" @click="load(true)">加载更多</button>
  </div>
</template>

<style scoped>
.history {
  display: flex;
  flex-direction: column;
  gap: 12px;
  max-width: 760px;
  margin: 0 auto;
}
.search {
  flex: 1;
}
.list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.item {
  padding: 10px 14px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--surface);
}
.meta {
  display: flex;
  align-items: center;
  gap: 10px;
}
.model {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 160px;
}
.source,
.result-text {
  margin: 6px 0 0;
  white-space: pre-wrap;
  word-break: break-word;
  user-select: text;
}
.result-text {
  font-family: var(--serif);
  font-size: 15px;
  line-height: 1.7;
}
.source {
  color: var(--muted);
  font-size: 13px;
  display: -webkit-box;
  -webkit-line-clamp: 3;
  line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.empty {
  text-align: center;
  padding: 48px 0;
}
.more {
  align-self: center;
}
</style>
