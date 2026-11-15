import React from 'react';
import { mergeSkipSegments, sameSegments, segmentsFromChapters } from '../playback/chapterSkips';
import { Chapter, SkipSegment } from '../types';

/**
 * The skip segments to show: the chapters of the playing file first, the online times for what
 * they leave out. The player status is read four times a second and brings a new chapter list each
 * time, so the result keeps the list it had while its content is the same; nothing that depends on
 * it is redone for nothing.
 */
export function useChapterSkips(chapters: Chapter[], durationMs: number, online: SkipSegment[]): SkipSegment[] {
  const merged = mergeSkipSegments(segmentsFromChapters(chapters, durationMs), online);
  const kept = React.useRef(merged);
  if (!sameSegments(kept.current, merged)) kept.current = merged;
  return kept.current;
}
