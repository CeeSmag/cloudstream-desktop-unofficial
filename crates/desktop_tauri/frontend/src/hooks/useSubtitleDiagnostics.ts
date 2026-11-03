import React from 'react';
import { MediaTrack, Preferences } from '../types';
import { logEvent } from '../utils/logger';
import { languageMatches } from '../utils/subtitleFlags';

interface SubtitleView {
  tracks: MediaTrack[];
  preferences: Preferences | null;
  /** Codes of the languages toggled in the Sources filter. */
  filter: string[];
  shownSources: number;
  totalSources: number;
}

/** How many of the tracks are in each preferred language: `eng=12, ara=4`. */
function preferredTally(tracks: MediaTrack[], preferences: Preferences): string {
  return preferences.preferred_subtitles
    .map((code) => {
      const language = preferences.languages.find((option) => option.code === code);
      const count = language
        ? tracks.filter((track) => languageMatches(track.language || track.title, language)).length
        : 0;
      return `${code}=${count}`;
    })
    .join(', ');
}

/**
 * Writes to the log what the viewer is offered: how many subtitle tracks the stream has in each
 * preferred language, and what the Sources filter shows. Logged only when something changed, so a
 * language that appears sometimes and not other times can be traced.
 */
export function useSubtitleDiagnostics({ tracks, preferences, filter, shownSources, totalSources }: SubtitleView): void {
  const subtitles = React.useMemo(() => tracks.filter((track) => track.kind === 'subtitle'), [tracks]);
  const tally = preferences ? preferredTally(subtitles, preferences) : 'preferences not loaded';

  React.useEffect(() => {
    logEvent('info', `subtitle menu: ${subtitles.length} tracks; preferred ${tally}`);
  }, [subtitles.length, tally]);

  React.useEffect(() => {
    logEvent(
      'info',
      `sources filter [${filter.join(', ')}] shows ${shownSources} of ${totalSources} sources`,
    );
  }, [filter, shownSources, totalSources]);
}
