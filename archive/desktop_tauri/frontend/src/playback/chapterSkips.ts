import { Chapter, SkipSegment } from '../types';

type SkipKind = SkipSegment['kind'];

/** A chapter shorter than this is a label, not a part worth a Skip button. */
export const MIN_SKIPPABLE_MS = 5_000;

/** Words that name a part wherever they stand in a chapter title (a number may follow: "Opening 2"). */
const NAMING_WORDS: Array<[SkipKind, RegExp]> = [
  ['intro', /\b(?:opening|intro)\d*\b/],
  ['credits', /\b(?:ending|outro|credits)\d*\b/],
  ['recap', /\b(?:recap|previously)\b/],
  ['preview', /\b(?:preview|next time)\b/],
];

/** Short forms such as "OP" or "ED 2" count only as the whole title: inside a longer one they are often a name. */
const WHOLE_TITLES: Array<[SkipKind, RegExp]> = [
  ['intro', /^op\s?\d*$/],
  ['credits', /^ed\s?\d*$/],
];

/** What a chapter title says the part is, or null for an ordinary chapter ("Part A", "Chapter 3"). */
export function kindOfTitle(title: string): SkipKind | null {
  const text = title
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, ' ')
    .trim();
  const whole = WHOLE_TITLES.find(([, pattern]) => pattern.test(text));
  if (whole) return whole[0];
  let best: { kind: SkipKind; index: number } | null = null;
  for (const [kind, pattern] of NAMING_WORDS) {
    const index = text.search(pattern);
    // The first word of the title decides: "Opening Credits" is an intro.
    if (index >= 0 && (best === null || index < best.index)) best = { kind, index };
  }
  return best ? best.kind : null;
}

/** Two neighbouring segments of one kind that touch become one. */
function joinNeighbours(segments: SkipSegment[]): SkipSegment[] {
  const joined: SkipSegment[] = [];
  for (const segment of segments) {
    const last = joined[joined.length - 1];
    if (last && last.kind === segment.kind && last.end_ms === segment.start_ms) {
      joined[joined.length - 1] = { ...last, end_ms: segment.end_ms };
    } else {
      joined.push(segment);
    }
  }
  return joined;
}

/**
 * The parts of the video that its own chapters name: a chapter runs until the next one starts (the
 * last until the end of the video, once that is known). No network, no ids: any file with chapters
 * called "Opening" or "Ending" gets its buttons.
 */
export function segmentsFromChapters(chapters: Chapter[], durationMs: number): SkipSegment[] {
  const ordered = [...chapters].sort((a, b) => a.time_ms - b.time_ms);
  const found: SkipSegment[] = [];
  ordered.forEach((chapter, index) => {
    const kind = kindOfTitle(chapter.title);
    const next = ordered[index + 1]?.time_ms ?? (durationMs > 0 ? durationMs : null);
    if (kind === null || next === null) return;
    const end = durationMs > 0 ? Math.min(next, durationMs) : next;
    if (end - chapter.time_ms < MIN_SKIPPABLE_MS) return;
    found.push({ kind, start_ms: chapter.time_ms, end_ms: end });
  });
  return joinNeighbours(found);
}

/** Whether two lists describe the same parts, so a re-read of the chapters can keep the list it had. */
export function sameSegments(a: SkipSegment[], b: SkipSegment[]): boolean {
  return (
    a.length === b.length &&
    a.every((segment, i) => {
      const other = b[i];
      return (
        other !== undefined &&
        segment.kind === other.kind &&
        segment.start_ms === other.start_ms &&
        segment.end_ms === other.end_ms
      );
    })
  );
}

/**
 * The segments to offer: the chapters decide for every kind they name (they describe this very
 * file), the online times fill in the kinds the chapters do not cover.
 */
export function mergeSkipSegments(fromChapters: SkipSegment[], online: SkipSegment[]): SkipSegment[] {
  const covered = new Set(fromChapters.map((segment) => segment.kind));
  const rest = online.filter((segment) => !covered.has(segment.kind));
  return [...fromChapters, ...rest].sort((a, b) => a.start_ms - b.start_ms);
}
