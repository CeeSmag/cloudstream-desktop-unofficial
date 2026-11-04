import React from 'react';
import { api } from '../services/api';
import { MediaDetails } from '../types';
import { logEvent } from '../utils/logger';

/**
 * The details of a title, loaded in the background for something that can already be shown
 * without them. Null while loading and when loading fails (which is logged, not shown).
 */
export function useTitleDetails(provider: string, url: string): MediaDetails | null {
  const [details, setDetails] = React.useState<MediaDetails | null>(null);

  React.useEffect(() => {
    let current = true;
    setDetails(null);
    api
      .getDetails(provider, url)
      .then((loaded) => {
        if (current) setDetails(loaded);
      })
      .catch((error) => logEvent('warn', `cannot load the details of ${provider}: ${String(error)}`));
    return () => {
      current = false;
    };
  }, [provider, url]);

  return details;
}
