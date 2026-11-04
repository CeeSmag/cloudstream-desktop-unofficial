import React from 'react';
import { api } from '../services/api';
import { MediaDetails } from '../types';
import { createMemoryCache } from '../utils/detailsCache';
import { logEvent } from '../utils/logger';

/** Titles already loaded in this run, so coming back to a page shows them at once. */
const loaded = createMemoryCache<MediaDetails>(30);

interface DetailsState {
  key: string;
  details: MediaDetails | null;
  /** Loading was tried and failed; the page then shows what it already knows. */
  failed: boolean;
}

/**
 * The details of a title, loaded in the background for something that can already be shown
 * without them. `details` is null while loading and when loading failed (`failed`, logged only).
 * A title loaded before comes back immediately, with no loading state in between.
 */
export function useTitleDetails(provider: string, url: string): { details: MediaDetails | null; failed: boolean } {
  const key = `${provider}\n${url}`;
  const [state, setState] = React.useState<DetailsState>({ key, details: loaded.get(key) ?? null, failed: false });
  const current: DetailsState = state.key === key ? state : { key, details: loaded.get(key) ?? null, failed: false };

  React.useEffect(() => {
    if (loaded.get(key)) return undefined;
    let live = true;
    api
      .getDetails(provider, url)
      .then((details) => {
        loaded.set(key, details);
        if (live) setState({ key, details, failed: false });
      })
      .catch((error) => {
        logEvent('warn', `cannot load the details of ${provider}: ${String(error)}`);
        if (live) setState({ key, details: null, failed: true });
      });
    return () => {
      live = false;
    };
  }, [key, provider, url]);

  return { details: current.details, failed: current.failed };
}
