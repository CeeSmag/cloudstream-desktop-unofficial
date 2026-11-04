import React from 'react';
import { api } from '../services/api';
import { canLookUpSkips, skipRequest } from '../playback/skipRequest';
import { Episode, MediaDetails, SkipSegment } from '../types';
import { logEvent } from '../utils/logger';

/**
 * The intro, recap and credits of the video that is playing. They are looked up once, as soon as
 * the length of the video is known; until then, and when nobody knows them, the list is empty.
 */
export function useSkipSegments(
  playing: { details: MediaDetails; episode: Episode } | null,
  durationMs: number,
): SkipSegment[] {
  const [found, setFound] = React.useState<{ key: string; segments: SkipSegment[] } | null>(null);
  const key = playing ? `${playing.details.url}\n${playing.episode.data}` : null;
  const known = key !== null && found?.key === key;
  const ready = playing !== null && canLookUpSkips(playing.details, durationMs);

  React.useEffect(() => {
    if (!playing || key === null || known || !ready) return undefined;
    let current = true;
    api
      .getSkipSegments(skipRequest(playing.details, playing.episode, durationMs))
      .then((segments) => {
        if (current) setFound({ key, segments });
        logEvent('info', `skip times for ${playing.episode.name}: ${segments.length} found`);
      })
      .catch((error) => logEvent('warn', `cannot look up skip times: ${String(error)}`));
    return () => {
      current = false;
    };
    // The lookup runs once per video: when it can first be asked, not each time the length is re-read.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, known, ready]);

  return known && found ? found.segments : [];
}
