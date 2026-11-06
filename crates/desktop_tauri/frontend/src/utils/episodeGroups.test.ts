import { describe, expect, it } from 'vitest';
import {
  CHUNK_SIZE,
  MACRO_CHUNK_COUNT,
  chunkEpisodes,
  chunkLabel,
  groupBySeason,
  groupChunksIntoMacro,
  macroGroupLabel,
  seasonLabel,
  seasonNumbers,
  sortEpisodes,
} from './episodeGroups';
import { Episode } from '../types';

const episode = (season: number | null, number: number | null): Episode => ({
  id: `${season}-${number}`,
  data: `${season}-${number}`,
  name: `Episode ${number}`,
  season,
  episode: number,
  poster_url: null,
  description: null,
});

describe('episode groups', () => {
  it('groups by season and puts episodes without a season in season 1', () => {
    const groups = groupBySeason([episode(2, 1), episode(null, 1), episode(1, 2)]);
    expect(groups.get(1)?.length).toBe(2);
    expect(groups.get(2)?.length).toBe(1);
  });

  it('lists regular seasons in order and specials last', () => {
    const groups = groupBySeason([episode(0, 1), episode(3, 1), episode(1, 1)]);
    expect(seasonNumbers(groups)).toEqual([1, 3, 0]);
    expect(seasonLabel(0)).toBe('Specials');
    expect(seasonLabel(3)).toBe('Season 3');
  });

  it('sorts by episode number, either way, with unnumbered episodes last', () => {
    const list = [episode(1, 3), episode(1, null), episode(1, 1)];
    expect(sortEpisodes(list, true).map((e) => e.episode)).toEqual([1, 3, null]);
    expect(sortEpisodes(list, false).map((e) => e.episode)).toEqual([null, 3, 1]);
  });

  it('splits long seasons into ranges labelled by episode number', () => {
    const list = Array.from({ length: CHUNK_SIZE + 5 }, (_, i) => episode(1, i + 1));
    const chunks = chunkEpisodes(list);
    expect(chunks.map((chunk) => chunk.length)).toEqual([CHUNK_SIZE, 5]);
    expect(chunkLabel(chunks[0] ?? [], 0)).toBe(`1–${CHUNK_SIZE}`);
    expect(chunkLabel(chunks[1] ?? [], 1)).toBe(`${CHUNK_SIZE + 1}–${CHUNK_SIZE + 5}`);
  });

  it('groups 20-episode sub-chunks into 100-episode macro-groups for long-running shows', () => {
    const list = Array.from({ length: 1410 }, (_, i) => episode(1, i + 1));
    const chunks = chunkEpisodes(list);
    const macroGroups = groupChunksIntoMacro(chunks);
    expect(chunks.length).toBe(71);
    expect(macroGroups.length).toBe(15);
    expect(macroGroups[0]?.length).toBe(MACRO_CHUNK_COUNT);
    expect(macroGroups[14]?.length).toBe(1);
    expect(macroGroupLabel(macroGroups[0] ?? [], 0)).toBe('1–100');
    expect(macroGroupLabel(macroGroups[1] ?? [], 1)).toBe('101–200');
    expect(macroGroupLabel(macroGroups[14] ?? [], 14)).toBe('1401–1410');
  });
});
