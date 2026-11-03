import React from 'react';
import { api } from '../services/api';
import { Preferences } from '../types';

/** How long the "Saved" note stays after a change. */
const SAVED_NOTE_MS = 2000;

/** The viewer's preferences (for now the preferred audio language) and how to change them. */
export function usePreferences(report: (error: unknown) => void) {
  const [preferences, setPreferences] = React.useState<Preferences | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [saved, setSaved] = React.useState(false);
  const savedTimer = React.useRef<number | null>(null);

  React.useEffect(() => {
    api.getPreferences().then(setPreferences).catch(report);
    return () => {
      if (savedTimer.current !== null) window.clearTimeout(savedTimer.current);
    };
  }, [report]);

  /** Saves the preferred audio language; `null` means the stream's own default track. */
  const chooseAudio = async (code: string | null) => {
    setBusy(true);
    try {
      setPreferences(await api.setPreferredAudio(code));
      setSaved(true);
      if (savedTimer.current !== null) window.clearTimeout(savedTimer.current);
      savedTimer.current = window.setTimeout(() => setSaved(false), SAVED_NOTE_MS);
    } catch (error) {
      report(error);
    } finally {
      setBusy(false);
    }
  };

  return { preferences, busy, saved, chooseAudio };
}
