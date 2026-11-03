import { MenuOption } from '../components/OptionMenu';
import { MediaTrack, Preferences } from '../types';
import { flagOfPreferredRank, menuFlags, preferredRankOfTrack } from './subtitleFlags';
import { numberDuplicateLabels } from './trackLabels';

const NOT_PREFERRED = Number.MAX_SAFE_INTEGER;

/**
 * The menu rows for the audio or subtitle tracks of the stream. Subtitles in the viewer's
 * preferred languages come first, in the order the viewer chose them, with a flag; the rest keep
 * their order after them.
 */
export function trackOptions(
  tracks: MediaTrack[],
  kind: MediaTrack['kind'],
  preferences: Preferences | null = null,
): MenuOption[] {
  const ofKind = tracks.filter((track) => track.kind === kind);
  const labels = numberDuplicateLabels(ofKind.map((track) => track.title));
  const rows = ofKind.map((track, position) => ({
    track,
    position,
    label: labels[position] ?? track.title,
    rank: kind === 'subtitle' && preferences ? preferredRankOfTrack(track, preferences) : -1,
  }));
  const rankOf = (row: { rank: number }) => (row.rank < 0 ? NOT_PREFERRED : row.rank);
  rows.sort((a, b) => rankOf(a) - rankOf(b) || a.position - b.position);
  return rows.map((row) => {
    const flag = preferences && row.rank >= 0 ? flagOfPreferredRank(row.rank, preferences) : null;
    return {
      id: row.track.id,
      label: row.label,
      selected: row.track.selected,
      flags: flag ? menuFlags([flag]) : undefined,
    };
  });
}
