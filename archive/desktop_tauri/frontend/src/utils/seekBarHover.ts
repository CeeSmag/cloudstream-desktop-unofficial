import { Chapter, SkipSegment } from '../types';
import { seekPositionFromClick } from './playerTime';

/** What the viewer is pointing at on the seek bar. */
export interface HoverInfo {
  /** The place in the video under the pointer, in milliseconds. */
  positionMs: number;
  /** Where the tooltip sits along the bar, 0 to 100 (kept off the very ends so it stays on screen). */
  percent: number;
  /** The kind of stretch under the pointer ("Intro", "Credits", ...), or null. */
  label: string | null;
  /** The name of the chapter under the pointer, or null when the stream has none. */
  chapter: string | null;
}

const LABELS: Record<SkipSegment['kind'], string> = {
  intro: 'Intro',
  recap: 'Recap',
  credits: 'Credits',
  preview: 'Preview',
};

/** Keeps the tooltip's middle this far (in percent) from either end of the bar. */
export const TOOLTIP_EDGE_PERCENT = 4;

/** The segment that contains `positionMs`: its start counts, its end does not. */
export function segmentAt(segments: SkipSegment[], positionMs: number): SkipSegment | null {
  return segments.find((s) => positionMs >= s.start_ms && positionMs < s.end_ms) ?? null;
}

/** The chapter that contains `positionMs`: the last one that has started by then. */
export function chapterAt(chapters: Chapter[], positionMs: number): Chapter | null {
  let found: Chapter | null = null;
  for (const chapter of chapters) {
    if (chapter.time_ms <= positionMs) found = chapter;
  }
  return found;
}

/** Where chapters start along the bar, 0 to 100, to draw dividers; none unless there are two or more chapters. */
export function chapterNotches(chapters: Chapter[], durationMs: number): number[] {
  if (durationMs <= 0 || chapters.length < 2) return [];
  const notches = chapters
    .filter((chapter) => chapter.time_ms > 0 && chapter.time_ms < durationMs)
    .map((chapter) => (chapter.time_ms / durationMs) * 100);
  return [...new Set(notches)];
}

/** The time, tooltip place and segment under a pointer at `clientX` over a bar at `left` with `width`. */
export function hoverInfo(input: {
  clientX: number;
  left: number;
  width: number;
  durationMs: number;
  segments: SkipSegment[];
  chapters?: Chapter[];
}): HoverInfo | null {
  const { clientX, left, width, durationMs, segments, chapters = [] } = input;
  if (durationMs <= 0 || width <= 0) return null;
  const positionMs = seekPositionFromClick(clientX, left, width, durationMs);
  const raw = ((clientX - left) / width) * 100;
  const percent = Math.min(100 - TOOLTIP_EDGE_PERCENT, Math.max(TOOLTIP_EDGE_PERCENT, raw));
  const segment = segmentAt(segments, positionMs);
  return {
    positionMs,
    percent,
    label: segment ? LABELS[segment.kind] : null,
    chapter: chapterAt(chapters, positionMs)?.title ?? null,
  };
}

/** A coloured stretch drawn on the bar. */
export interface SegmentMark {
  kind: SkipSegment['kind'];
  /** Where it starts along the bar, 0 to 100. */
  left: number;
  /** How wide it is, in percent of the bar. */
  width: number;
}

/** The marks for the bar; stretches outside the video, empty ones and any with an unknown length are left out. */
export function segmentMarks(segments: SkipSegment[], durationMs: number): SegmentMark[] {
  if (durationMs <= 0) return [];
  const marks: SegmentMark[] = [];
  for (const segment of segments) {
    const start = Math.max(0, segment.start_ms);
    const end = Math.min(durationMs, segment.end_ms);
    if (end <= start) continue;
    marks.push({ kind: segment.kind, left: (start / durationMs) * 100, width: ((end - start) / durationMs) * 100 });
  }
  return marks;
}
