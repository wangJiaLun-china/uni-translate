import { createRouter, createWebHashHistory } from "vue-router";
import CardView from "./views/CardView.vue";

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    // The main window: the everyday translation card.
    { path: "/", component: CardView, meta: { bare: true } },
    // The hotkey popup window.
    { path: "/popup", component: () => import("./views/PopupView.vue"), meta: { bare: true } },
    // The settings/history window.
    { path: "/history", component: () => import("./views/HistoryView.vue") },
    { path: "/settings", component: () => import("./views/SettingsView.vue") },
  ],
});
