/** The pop-up shown when the viewer asks for the sources again. */
export const REFRESH_FEEDBACK = 'Searching for sources again';

/**
 * Whether the Refresh button of the Sources menu can be pressed: only once the video's opening is
 * over (`checking` is false; the backend refuses while no stream plays) and while no refresh is
 * starting.
 */
export function canRefreshSources(checking: boolean, refreshing: boolean): boolean {
  return !checking && !refreshing;
}

/** The button's tooltip, which says why it cannot be pressed. */
export function refreshHint(checking: boolean, refreshing: boolean): string {
  if (refreshing) return 'Starting a new search for sources…';
  if (checking) return 'The sources can be refreshed once the video plays';
  return 'Forget the saved sources and search and check them again; the video keeps playing';
}
