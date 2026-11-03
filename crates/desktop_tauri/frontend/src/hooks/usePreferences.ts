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

  /** Runs a save, then shows the new preferences and the "Saved" note. */
  const save = async (change: () => Promise<Preferences>) => {
    setBusy(true);
    try {
      setPreferences(await change());
      setSaved(true);
      if (savedTimer.current !== null) window.clearTimeout(savedTimer.current);
      savedTimer.current = window.setTimeout(() => setSaved(false), SAVED_NOTE_MS);
    } catch (error) {
      report(error);
    } finally {
      setBusy(false);
    }
  };

  /** Saves the preferred audio language; `null` means the stream's own default track. */
  const chooseAudio = (code: string | null) => save(() => api.setPreferredAudio(code));

  /** Saves the languages whose subtitles get a flag in the player (at most three). */
  const chooseSubtitles = (codes: string[]) => save(() => api.setPreferredSubtitles(codes));

  return { preferences, busy, saved, chooseAudio, chooseSubtitles };
}
