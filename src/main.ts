import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { createPinia } from "pinia";
import { createApp } from "vue";
import App from "./App.vue";
import { router } from "./router";
import { useSettingsStore } from "./stores/settings";
import "./styles/main.css";

const app = createApp(App).use(createPinia()).use(router);

// Both windows load the same page; the popup window gets its own route.
if (getCurrentWebviewWindow().label === "popup") {
  await router.replace("/popup");
} else {
  // Sent by the tray menu.
  void listen<string>("app://navigate", (event) => router.push(event.payload));
}

// The popup loads settings itself each time it is shown.
await useSettingsStore().load().catch((err) => console.error(err));
app.mount("#app");
