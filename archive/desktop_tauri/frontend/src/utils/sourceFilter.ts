import { Preferences, SourceInfo } from '../types';
import { offersAllSubtitles } from './subtitleFlags';

/**
 * The sources that offer subtitles in every wanted language (codes from the preferences). With
 * nothing wanted every source is kept. The source that is playing always stays listed.
 */
export function filterSourcesBySubtitles(
  sources: SourceInfo[],
  preferences: Preferences | null,
  wanted: string[],
): SourceInfo[] {
  if (!preferences || wanted.length === 0) return sources;
  return sources.filter(
    (source) => source.selected || offersAllSubtitles(source.subtitle_languages, preferences, wanted),
  );
}
