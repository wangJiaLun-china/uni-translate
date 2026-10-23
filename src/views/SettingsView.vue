<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import * as api from "../api/commands";
import HotkeyInput from "../components/HotkeyInput.vue";
import Icon from "../components/Icon.vue";
import { speakText } from "../composables/useSpeaker";
import { toast } from "../composables/useToast";
import { useSettingsStore } from "../stores/settings";
import type { KeyKind, Settings } from "../types";

const store = useSettingsStore();
const form = ref<Settings | null>(null);
/** Keys typed into the form; empty means "keep the saved key". */
const KEY_KINDS: KeyKind[] = ["chat", "tts", "baidu_api_key", "baidu_secret_key"];
const keys = reactive<Record<KeyKind, string>>({ chat: "", tts: "", baidu_api_key: "", baidu_secret_key: "" });
const hasKey = reactive<Record<KeyKind, boolean>>({
  chat: false,
  tts: false,
  baidu_api_key: false,
  baidu_secret_key: false,
});
const models = ref<string[]>([]);
const saving = ref(false);
const loadingModels = ref(false);
const testing = ref(false);
const testResult = ref<{ ok: boolean; message: string } | null>(null);

const dirty = computed(
  () =>
    !!form.value &&
    (JSON.stringify(form.value) !== JSON.stringify(store.settings) || KEY_KINDS.some((kind) => !!keys[kind].trim())),
);

/** Common voices from Baidu's docs; premium ones must be enabled in the console first. */
const BAIDU_VOICES = [
  { id: 0, name: "度小美（基础）" },
  { id: 1, name: "度小宇（基础）" },
  { id: 3, name: "度逍遥（基础）" },
  { id: 4, name: "度丫丫（基础）" },
  { id: 5003, name: "度逍遥（精品）" },
  { id: 5118, name: "度小鹿（精品）" },
  { id: 106, name: "度博文（精品）" },
  { id: 110, name: "度小童（精品）" },
  { id: 111, name: "度小萌（精品）" },
  { id: 103, name: "度米朵（精品）" },
  { id: 5, name: "度小娇（精品）" },
];
const customVoice = ref(false);
const voiceChoice = computed<number | "custom">({
  get: () => {
    const id = form.value?.tts.baiduVoice ?? 0;
    return customVoice.value || !BAIDU_VOICES.some((v) => v.id === id) ? "custom" : id;
  },
  set: (choice) => {
    customVoice.value = choice === "custom";
    if (choice !== "custom" && form.value) form.value.tts.baiduVoice = choice;
  },
});

function openConsole() {
  void api.openBaiduConsole().catch((err) => toast(api.errorMessage(err), "error"));
}

/** A plain copy; `structuredClone` rejects the store's reactive proxies. */
const clone = <T,>(value: T): T => JSON.parse(JSON.stringify(value));

async function reload() {
  try {
    await store.load();
    form.value = clone(store.settings!);
    const saved = await Promise.all(KEY_KINDS.map((kind) => api.hasApiKey(kind)));
    KEY_KINDS.forEach((kind, i) => (hasKey[kind] = saved[i]));
  } catch (err) {
    toast(api.errorMessage(err), "error");
  }
}

onMounted(reload);

async function save(): Promise<boolean> {
  if (!form.value) return false;
  saving.value = true;
  try {
    const saved = await store.save(form.value);
    form.value = clone(saved);
    for (const kind of KEY_KINDS) {
      if (keys[kind].trim()) {
        await api.setApiKey(kind, keys[kind]);
        keys[kind] = "";
        hasKey[kind] = true;
      }
    }
    toast("已保存");
    return true;
  } catch (err) {
    toast(api.errorMessage(err), "error");
    return false;
  } finally {
    saving.value = false;
  }
}

async function clearKey(kind: KeyKind) {
  try {
    await api.setApiKey(kind, "");
    keys[kind] = "";
    hasKey[kind] = false;
    toast("已清除 API Key");
  } catch (err) {
    toast(api.errorMessage(err), "error");
  }
}

async function fetchModels() {
  if (!form.value) return;
  loadingModels.value = true;
  try {
    models.value = await api.listModels(form.value.baseUrl, keys.chat);
    toast(models.value.length ? `获取到 ${models.value.length} 个模型` : "接口没有返回模型，请手动填写");
  } catch (err) {
    toast(`获取模型列表失败：${api.errorMessage(err)}`, "error");
  } finally {
    loadingModels.value = false;
  }
}

async function test() {
  if (!form.value) return;
  testing.value = true;
  testResult.value = null;
  try {
    const reply = await api.testConnection(form.value.baseUrl, form.value.model, keys.chat);
    testResult.value = { ok: true, message: `连接成功：「${reply}」` };
  } catch (err) {
    testResult.value = { ok: false, message: api.errorMessage(err) };
  } finally {
    testing.value = false;
  }
}

async function previewVoice() {
  if (dirty.value && !(await save())) return;
  void speakText("你好，这是一段语音测试。Hello, this is a voice test.", "settings-preview");
}
</script>

<template>
  <div v-if="form" class="settings">
    <section class="card">
      <h2>翻译接口</h2>
      <p class="muted small">支持任何 OpenAI 兼容接口，如 DeepSeek、通义千问、智谱、Kimi、豆包、Ollama 等。</p>

      <label class="field">
        <span>接口地址（Base URL）</span>
        <input v-model="form.baseUrl" class="input" placeholder="例如 https://api.deepseek.com/v1" spellcheck="false" />
      </label>

      <label class="field">
        <span>API Key</span>
        <div class="row">
          <input
            v-model="keys.chat"
            class="input"
            type="password"
            autocomplete="off"
            :placeholder="hasKey.chat ? '已保存（留空则不修改）' : '本地模型可以不填'"
          />
          <button v-if="hasKey.chat" class="btn" @click="clearKey('chat')">清除</button>
        </div>
      </label>

      <label class="field">
        <span>模型</span>
        <div class="row">
          <input v-model="form.model" class="input" list="model-options" placeholder="例如 deepseek-chat" spellcheck="false" />
          <datalist id="model-options">
            <option v-for="m in models" :key="m" :value="m" />
          </datalist>
          <button class="btn" :disabled="!form.baseUrl || loadingModels" @click="fetchModels">
            <Icon v-if="loadingModels" name="loader" /> 获取模型列表
          </button>
        </div>
      </label>

      <label class="field">
        <span>温度（Temperature）：{{ form.temperature.toFixed(1) }}</span>
        <input v-model.number="form.temperature" type="range" min="0" max="1.5" step="0.1" />
      </label>

      <div class="row">
        <button class="btn" :disabled="!form.baseUrl || !form.model || testing" @click="test">
          <Icon v-if="testing" name="loader" /> 测试连接
        </button>
        <span v-if="testResult" class="small" :class="testResult.ok ? 'ok-text' : 'error-text'">
          {{ testResult.message }}
        </span>
      </div>
    </section>

    <section class="card">
      <h2>语音朗读</h2>
      <div class="segmented" role="radiogroup" aria-label="语音服务">
        <label :class="{ on: form.tts.provider === 'openai' }">
          <input v-model="form.tts.provider" type="radio" value="openai" />
          OpenAI 兼容接口
        </label>
        <label :class="{ on: form.tts.provider === 'baidu' }">
          <input v-model="form.tts.provider" type="radio" value="baidu" />
          百度语音
        </label>
      </div>

      <template v-if="form.tts.provider === 'openai'">
        <p class="muted small">使用 OpenAI 兼容的 <code>/audio/speech</code> 接口，返回 MP3 音频。</p>

        <label class="check">
          <input v-model="form.tts.useSameEndpoint" type="checkbox" />
          复用翻译接口的地址和 API Key
        </label>

        <template v-if="!form.tts.useSameEndpoint">
          <label class="field">
            <span>语音接口地址（Base URL）</span>
            <input v-model="form.tts.baseUrl" class="input" placeholder="例如 https://api.openai.com/v1" spellcheck="false" />
          </label>
          <label class="field">
            <span>语音接口 API Key</span>
            <div class="row">
              <input
                v-model="keys.tts"
                class="input"
                type="password"
                autocomplete="off"
                :placeholder="hasKey.tts ? '已保存（留空则不修改）' : ''"
              />
              <button v-if="hasKey.tts" class="btn" @click="clearKey('tts')">清除</button>
            </div>
          </label>
        </template>

        <div class="grid-2">
          <label class="field">
            <span>语音模型</span>
            <input v-model="form.tts.model" class="input" placeholder="例如 tts-1" spellcheck="false" />
          </label>
          <label class="field">
            <span>音色</span>
            <input v-model="form.tts.voice" class="input" placeholder="例如 alloy" spellcheck="false" />
          </label>
        </div>
      </template>

      <template v-else>
        <p class="muted small">
          使用百度智能云的短文本在线合成。还没有 Key？
          <a href="https://console.bce.baidu.com/ai-engine/speech/overview/" @click.prevent="openConsole">
            去百度智能云申请
          </a>
        </p>

        <label class="field">
          <span>API Key</span>
          <div class="row">
            <input
              v-model="keys.baidu_api_key"
              class="input"
              type="password"
              autocomplete="off"
              spellcheck="false"
              :placeholder="hasKey.baidu_api_key ? '已保存（留空则不修改）' : ''"
            />
            <button v-if="hasKey.baidu_api_key" class="btn" @click="clearKey('baidu_api_key')">清除</button>
          </div>
        </label>
        <label class="field">
          <span>Secret Key</span>
          <div class="row">
            <input
              v-model="keys.baidu_secret_key"
              class="input"
              type="password"
              autocomplete="off"
              spellcheck="false"
              :placeholder="hasKey.baidu_secret_key ? '已保存（留空则不修改）' : ''"
            />
            <button v-if="hasKey.baidu_secret_key" class="btn" @click="clearKey('baidu_secret_key')">清除</button>
          </div>
        </label>

        <div class="grid-2">
          <label class="field">
            <span>音色</span>
            <select v-model="voiceChoice" class="select">
              <option v-for="voice in BAIDU_VOICES" :key="voice.id" :value="voice.id">{{ voice.name }}</option>
              <option value="custom">其他（填写音色 ID）</option>
            </select>
          </label>
          <label v-if="voiceChoice === 'custom'" class="field">
            <span>音色 ID</span>
            <input v-model.number="form.tts.baiduVoice" class="input" type="number" min="0" step="1" />
          </label>
        </div>
        <p class="muted small">精品音色需要先在百度智能云控制台开通。</p>
      </template>

      <label class="field">
        <span>语速：{{ form.tts.speed.toFixed(2) }}×</span>
        <input v-model.number="form.tts.speed" type="range" min="0.5" max="2" step="0.05" />
      </label>

      <div class="row">
        <button class="btn" @click="previewVoice"><Icon name="speaker" /> 试听</button>
      </div>
    </section>

    <section class="card">
      <h2>通用</h2>
      <div class="grid-2">
        <label class="field">
          <span>默认目标语言</span>
          <select v-model="form.targetLang" class="select">
            <option v-for="lang in store.languages" :key="lang.code" :value="lang.code">{{ lang.name }}</option>
          </select>
        </label>
        <label class="field">
          <span>第二目标语言</span>
          <select v-model="form.secondaryLang" class="select">
            <option v-for="lang in store.languages" :key="lang.code" :value="lang.code">{{ lang.name }}</option>
          </select>
        </label>
      </div>
      <p class="muted small">原文已经是目标语言时，自动翻译成第二目标语言。</p>

      <label class="field">
        <span>划词翻译快捷键</span>
        <HotkeyInput v-model="form.hotkey" />
      </label>
      <p class="muted small">
        选中任意文字后按下快捷键即可翻译。macOS 需要在「系统设置 → 隐私与安全性 → 辅助功能」中允许本应用。
      </p>

      <label class="check">
        <input v-model="form.historyEnabled" type="checkbox" />
        保存翻译历史
      </label>
    </section>

    <div class="save-bar">
      <span v-if="dirty" class="muted small">有未保存的修改</span>
      <span class="spacer" />
      <button class="btn" :disabled="!dirty || saving" @click="reload">还原</button>
      <button class="btn primary" :disabled="!dirty || saving" @click="save">
        <Icon :name="saving ? 'loader' : 'check'" /> 保存
      </button>
    </div>
  </div>
</template>

<style scoped>
.settings {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: 680px;
  margin: 0 auto;
  padding-bottom: 8px;
}
.card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 16px 18px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--surface);
}
h2 {
  margin: 0;
  font-size: 15px;
}
.card > p {
  margin: -6px 0 0;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 13px;
}
.row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.row .input {
  flex: 1;
}
.grid-2 {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 12px;
}
.check {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}
.save-bar {
  position: sticky;
  /* Cover the scroll container's bottom padding too. */
  bottom: -16px;
  margin-bottom: -16px;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 0 16px;
  border-top: 1px solid var(--border);
  background: var(--bg);
}
/* Two-way choice of speech service. */
.segmented {
  display: inline-flex;
  align-self: flex-start;
  padding: 2px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg);
}
.segmented label {
  padding: 4px 12px;
  border-radius: 6px;
  font-size: 13px;
  color: var(--muted);
  cursor: pointer;
}
.segmented label.on {
  background: var(--surface);
  color: var(--text);
  box-shadow: 0 0 0 1px var(--border);
}
.segmented label:has(input:focus-visible) {
  outline: 2px solid var(--accent);
}
.segmented input {
  position: absolute;
  opacity: 0;
  pointer-events: none;
}
code {
  font-family: ui-monospace, monospace;
  font-size: 12px;
}
</style>
