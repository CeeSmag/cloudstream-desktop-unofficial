import { Episode } from '../types';

/** How many episodes are listed at a time; longer seasons are split into ranges. */
export const CHUNK_SIZE = 20;

/** How many 20-episode sub-ranges belong to one 100-episode macro-group. */
export const MACRO_CHUNK_COUNT = 5;

/** Episodes without a season belong to season 1. */
export function groupBySeason(episodes: Episode[]): Map<number, Episode[]> {
  const groups = new Map<number, Episode[]>();
  for (const episode of episodes) {
    const season = episode.season ?? 1;
    groups.set(season, [...(groups.get(season) ?? []), episode]);
  }
  return groups;
}

/** Season numbers in order, with specials (season 0 and below) after the regular seasons. */
export function seasonNumbers(groups: Map<number, Episode[]>): number[] {
  const numbers = Array.from(groups.keys());
  const regular = numbers.filter((season) => season > 0).sort((a, b) => a - b);
  const specials = numbers.filter((season) => season <= 0).sort((a, b) => a - b);
  return [...regular, ...specials];
}

/** The name of a season tab: season 0 is "Specials". */
export function seasonLabel(season: number): string {
  return season <= 0 ? 'Specials' : `Season ${season}`;
}

/** Episodes in order of their number, oldest first or newest first. */
export function sortEpisodes(episodes: Episode[], ascending: boolean): Episode[] {
  const byNumber = [...episodes].sort(
    (a, b) => (a.episode ?? Number.MAX_SAFE_INTEGER) - (b.episode ?? Number.MAX_SAFE_INTEGER),
  );
  return ascending ? byNumber : byNumber.reverse();
}

/** Splits a long list into ranges of at most [CHUNK_SIZE] episodes. */
export function chunkEpisodes(episodes: Episode[]): Episode[][] {
  const chunks: Episode[][] = [];
  for (let start = 0; start < episodes.length; start += CHUNK_SIZE) {
    chunks.push(episodes.slice(start, start + CHUNK_SIZE));
  }
  return chunks;
}

/** The label of a range, such as "1–20": the numbers of its first and last episode. */
export function chunkLabel(chunk: Episode[], index: number): string {
  const first = chunk[0]?.episode ?? index * CHUNK_SIZE + 1;
  const last = chunk[chunk.length - 1]?.episode ?? index * CHUNK_SIZE + chunk.length;
  return `${first}–${last}`;
}

/** Groups 20-episode sub-chunks into macro-groups of at most [MACRO_CHUNK_COUNT] chunks (100 episodes). */
export function groupChunksIntoMacro(chunks: Episode[][]): Episode[][][] {
  const macroGroups: Episode[][][] = [];
  for (let start = 0; start < chunks.length; start += MACRO_CHUNK_COUNT) {
    macroGroups.push(chunks.slice(start, start + MACRO_CHUNK_COUNT));
  }
  return macroGroups;
}

/** The label of a 100-episode macro-group, such as "1–100": the numbers of its first and last episode. */
export function macroGroupLabel(macroChunks: Episode[][], macroIndex: number): string {
  const firstChunkIdx = macroIndex * MACRO_CHUNK_COUNT;
  const lastChunkIdx = firstChunkIdx + Math.max(macroChunks.length - 1, 0);
  const firstChunk = macroChunks[0] ?? [];
  const lastChunk = macroChunks[macroChunks.length - 1] ?? [];
  const first = firstChunk[0]?.episode ?? firstChunkIdx * CHUNK_SIZE + 1;
  const last = lastChunk[lastChunk.length - 1]?.episode ?? lastChunkIdx * CHUNK_SIZE + lastChunk.length;
  return `${first}–${last}`;
}
