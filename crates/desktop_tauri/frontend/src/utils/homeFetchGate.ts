/** Home / provider fetches wait until start-up reports ready (avoids a double load under StartupScreen). */
export function mayFetchHome(ready: boolean): boolean {
  return ready;
}

/** Delay before background tidy / update check so the first Home paint is not competing. */
export const POST_READY_STAGGER_MS = 45_000;
