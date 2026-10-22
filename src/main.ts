import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { createPinia } from "pinia";
import { createApp } from "vue";
import App from "./App.vue";
import { takeManageRoute } from "./api/commands";
import { router } from "./router";
import { useSettingsStore } from "./stores/settings";
import "./styles/main.css";

const app = createApp(App).use(createPinia()).use(router);

// All windows load the same page; the window label picks what it shows.
const label = getCurrentWebviewWindow().label;
if (label === "popup") {
  await router.replace("/popup");
} else if (label === "manage") {
  await router.replace((await takeManageRoute()) ?? "/settings");
  // Sent when the window is already open and the card or tray asks for a page.
  void listen<string>("app://navigate", (event) => router.push(event.payload));
} else {
  await router.replace("/");
}

await useSettingsStore().load().catch((err) => console.error(err));
app.mount("#app");
