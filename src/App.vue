<script setup lang="ts">
import { computed } from "vue";
import { useRoute } from "vue-router";
import Icon, { type IconName } from "./components/Icon.vue";
import ToastHost from "./components/ToastHost.vue";

const route = useRoute();
const bare = computed(() => !!route.meta.bare);

const tabs: { to: string; label: string; icon: IconName }[] = [
  { to: "/history", label: "历史记录", icon: "history" },
  { to: "/settings", label: "设置", icon: "settings" },
];
</script>

<template>
  <RouterView v-if="bare" />
  <div v-else class="shell">
    <header class="topbar">
      <span class="brand">uni-translate</span>
      <nav class="tabs">
        <RouterLink v-for="tab in tabs" :key="tab.to" :to="tab.to" class="tab">
          <Icon :name="tab.icon" /> {{ tab.label }}
        </RouterLink>
      </nav>
    </header>
    <main class="content">
      <RouterView />
    </main>
  </div>
  <ToastHost />
</template>

<style scoped>
.shell {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--bg);
}
.topbar {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 10px 16px;
  border-bottom: 1px solid var(--border);
  background: var(--surface);
}
.brand {
  font-weight: 600;
  margin-right: 8px;
}
.tabs {
  display: flex;
  gap: 4px;
}
.tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  border-radius: 8px;
  color: var(--muted);
  text-decoration: none;
  font-size: 13px;
}
.tab:hover {
  background: var(--hover);
  color: var(--text);
}
.tab.router-link-exact-active {
  background: var(--accent-soft);
  color: var(--accent);
}
.content {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: 16px;
}
</style>
