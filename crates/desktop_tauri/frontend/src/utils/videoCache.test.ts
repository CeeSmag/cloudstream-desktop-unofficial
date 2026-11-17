import { describe, expect, it } from 'vitest';
import { awayText, sizeLabel, splitMb, splitText } from './videoCache';

describe('the video cache setting', () => {
  it('names sizes in MB below a gigabyte and in GB from there on', () => {
    expect(sizeLabel(256)).toBe('256 MB');
    expect(sizeLabel(1024)).toBe('1 GB');
    expect(sizeLabel(1536)).toBe('1.5 GB');
  });

  it('splits the size by the ahead share, the parts adding up to it', () => {
    expect(splitMb(2048, 50)).toEqual([1024, 1024]);
    expect(splitMb(2048, 75)).toEqual([1536, 512]);
    const [ahead, behind] = splitMb(256, 90);
    expect(ahead + behind).toBe(256);
  });

  it('describes the split while playing and the halves while the video waits', () => {
    expect(splitText(2048, 75)).toBe('1.5 GB ahead, 512 MB behind');
    expect(awayText(2048, 75)).toBe('768 MB ahead, 256 MB behind');
  });
});
