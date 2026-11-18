import { listen } from '@tauri-apps/api/event';

/** Subscribes to an event from the Rust side; the returned function unsubscribes. */
export function onBackendEvent<T = void>(name: string, handler: (payload: T) => void): () => void {
  let stop: (() => void) | undefined;
  let cancelled = false;
  listen<T>(name, (event) => handler(event.payload))
    .then((unlisten) => {
      if (cancelled) unlisten();
      else stop = unlisten;
    })
    .catch(() => undefined);
  return () => {
    cancelled = true;
    stop?.();
  };
}
