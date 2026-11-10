import { setupWorker } from "msw/browser";
import { handlers } from "./handlers";

const isBrowser = typeof window !== "undefined" && typeof navigator !== "undefined" && "serviceWorker" in navigator;

export const worker = isBrowser ? setupWorker(...handlers) : null;

export async function initMocks() {
  if (isBrowser && worker) {
    await worker.start({
      onUnhandledRequest: "bypass",
    });
  }
}
