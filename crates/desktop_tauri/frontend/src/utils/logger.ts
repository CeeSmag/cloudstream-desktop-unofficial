/** Levels of the log lines the web app sends to the backend; they end up in `webapp-logs.txt`. */
export type LogLevel = 'error' | 'warn' | 'info' | 'debug' | 'trace';

/** Levels that are always sent, because they say something went wrong. */
const ALWAYS_SENT: LogLevel[] = ['error', 'warn'];

let debugLogging = false;

/** Called whenever the backend's debug logging switch is read or changed. */
export function setDebugLogging(enabled: boolean): void {
  debugLogging = enabled;
}

/** Whether detailed web app logs are being sent. */
export function isDebugLogging(): boolean {
  return debugLogging;
}

/** Whether a line of this level leaves the page: problems always, the rest only in debug mode. */
export function shouldSend(level: LogLevel, debugOn: boolean): boolean {
  return debugOn || ALWAYS_SENT.includes(level);
}

/** A readable text for anything that was thrown, rejected or logged. */
export function describeValue(value: unknown): string {
  if (typeof value === 'string') return value;
  if (value instanceof Error) return value.stack ?? value.message;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

/**
 * Sends one line to the backend log. While debug logging is off only warnings and errors are
 * sent, so the page does no extra work; a failing send is dropped (there is nowhere to report it).
 */
export function logEvent(level: LogLevel, message: string): void {
  if (!shouldSend(level, debugLogging)) return;
  if (typeof window === 'undefined' || !window.__TAURI_INTERNALS__) return;
  import('@tauri-apps/api/core')
    .then(({ invoke }) => invoke('log_from_webapp', { level, message }))
    .catch(() => undefined);
}

/** Records errors nobody caught, so they show up in the logs instead of vanishing in the console. */
export function installGlobalErrorLogging(): void {
  window.addEventListener('error', (event) => {
    logEvent('error', `uncaught error: ${event.message} (${event.filename}:${event.lineno})`);
  });
  window.addEventListener('unhandledrejection', (event) => {
    logEvent('error', `unhandled promise rejection: ${describeValue(event.reason)}`);
  });
}
