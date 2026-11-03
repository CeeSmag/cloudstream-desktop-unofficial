import React from 'react';
import { api } from '../services/api';
import { MediaTrack, Preferences } from '../types';
import { logEvent } from '../utils/logger';
import { SubtitleFlag, subtitleFlags } from '../utils/subtitleFlags';

/** The flags of the viewer's preferred subtitle languages that the playing stream offers. */
export function useSubtitleFlags(tracks: MediaTrack[]): SubtitleFlag[] {
  const [preferences, setPreferences] = React.useState<Preferences | null>(null);

  React.useEffect(() => {
    api
      .getPreferences()
      .then(setPreferences)
      .catch((error) => logEvent('warn', `cannot read the subtitle preferences: ${String(error)}`));
  }, []);

  return React.useMemo(() => (preferences ? subtitleFlags(tracks, preferences) : []), [tracks, preferences]);
}
