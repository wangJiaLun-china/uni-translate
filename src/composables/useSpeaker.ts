import { reactive } from "vue";
import * as api from "../api/commands";
import { toast } from "./useToast";

/** One player per window: starting playback anywhere stops the previous one. */
export const speaker = reactive({
  /** Which button is active, so only it shows a stop icon. */
  activeId: null as string | null,
  loading: false,
});

let generation = 0;
let audio: HTMLAudioElement | null = null;
let finishCurrent: (() => void) | null = null;

export function stopSpeaking() {
  generation++;
  audio?.pause();
  audio = null;
  finishCurrent?.();
  finishCurrent = null;
  speaker.activeId = null;
  speaker.loading = false;
}

/** Speaks `text` piece by piece, fetching the next piece while one plays. */
export async function speakText(text: string, id: string) {
  stopSpeaking();
  const gen = generation;
  speaker.activeId = id;
  speaker.loading = true;
  try {
    const segments = await api.splitTtsText(text);
    let next: Promise<ArrayBuffer> | null = segments.length ? api.speak(segments[0]) : null;
    for (let i = 0; next; i++) {
      const buffer = await next;
      if (gen !== generation) return;
      next = i + 1 < segments.length ? api.speak(segments[i + 1]) : null;
      next?.catch(() => {}); // surfaced when awaited
      speaker.loading = false;
      await play(buffer, gen);
      if (gen !== generation) return;
    }
  } catch (err) {
    if (gen === generation) toast(`朗读失败：${api.errorMessage(err)}`, "error");
  } finally {
    if (gen === generation) {
      speaker.activeId = null;
      speaker.loading = false;
    }
  }
}

function play(buffer: ArrayBuffer, gen: number): Promise<void> {
  const url = URL.createObjectURL(new Blob([buffer], { type: "audio/mpeg" }));
  return new Promise<void>((resolve, reject) => {
    if (gen !== generation) return resolve();
    const element = new Audio(url);
    audio = element;
    finishCurrent = resolve;
    element.onended = () => resolve();
    element.onerror = () => reject(new Error("无法播放返回的音频"));
    element.play().catch(reject);
  }).finally(() => URL.revokeObjectURL(url));
}
