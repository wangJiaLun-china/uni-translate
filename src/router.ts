import { createRouter, createWebHashHistory } from "vue-router";
import TranslateView from "./views/TranslateView.vue";

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", component: TranslateView },
    { path: "/history", component: () => import("./views/HistoryView.vue") },
    { path: "/settings", component: () => import("./views/SettingsView.vue") },
    // The hotkey popup window: no navigation chrome.
    { path: "/popup", component: () => import("./views/PopupView.vue"), meta: { bare: true } },
  ],
});
