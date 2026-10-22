import { LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { onBeforeUnmount, onMounted, type Ref } from "vue";

/**
 * Keeps the window exactly as tall as `element`, so the card grows with the
 * translation. The width stays wherever the user put it. Cap the element's
 * height in CSS; past that, its content should scroll.
 */
export function useAutoHeight(element: Ref<HTMLElement | null>) {
  const win = getCurrentWebviewWindow();
  let observer: ResizeObserver | null = null;
  let frame = 0;

  async function fit() {
    if (!element.value) return;
    const height = Math.ceil(element.value.getBoundingClientRect().height);
    const scale = await win.scaleFactor();
    const size = (await win.innerSize()).toLogical(scale);
    if (Math.abs(size.height - height) > 1) {
      await win.setSize(new LogicalSize(size.width, height));
    }
  }

  onMounted(() => {
    observer = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => void fit().catch((err) => console.error(err)));
    });
    if (element.value) observer.observe(element.value);
  });

  onBeforeUnmount(() => {
    cancelAnimationFrame(frame);
    observer?.disconnect();
  });
}
