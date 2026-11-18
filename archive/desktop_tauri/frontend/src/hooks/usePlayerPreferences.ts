import React from 'react';
import { api } from '../services/api';
import { Preferences } from '../types';
import { logEvent } from '../utils/logger';

/** The viewer's preferences as the player needs them (which subtitle languages get a flag); null until loaded. */
export function usePlayerPreferences(): Preferences | null {
  const [preferences, setPreferences] = React.useState<Preferences | null>(null);

  React.useEffect(() => {
    api
      .getPreferences()
      .then(setPreferences)
      .catch((error) => logEvent('warn', `cannot read the preferences: ${String(error)}`));
  }, []);

  return preferences;
}
