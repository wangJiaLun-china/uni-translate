import { reactive } from "vue";

export interface Toast {
  id: number;
  message: string;
  kind: "info" | "error";
}

export const toasts = reactive<Toast[]>([]);
let nextId = 0;

export function toast(message: string, kind: Toast["kind"] = "info") {
  const id = nextId++;
  toasts.push({ id, message, kind });
  setTimeout(
    () => {
      const index = toasts.findIndex((t) => t.id === id);
      if (index >= 0) toasts.splice(index, 1);
    },
    kind === "error" ? 5000 : 2000,
  );
}
