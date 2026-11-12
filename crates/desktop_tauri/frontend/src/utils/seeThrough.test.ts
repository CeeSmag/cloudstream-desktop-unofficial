import { describe, expect, it } from 'vitest';
import { PAINT_PROOF_MS, videoHasPainted } from './seeThrough';

describe('videoHasPainted', () => {
  it('is false until the position has moved enough', () => {
    expect(videoHasPainted(5_000, 5_000)).toBe(false);
    expect(videoHasPainted(5_000, 5_000 + PAINT_PROOF_MS - 1)).toBe(false);
  });

  it('is true once the position moved by the proof distance', () => {
    expect(videoHasPainted(5_000, 5_000 + PAINT_PROOF_MS)).toBe(true);
  });

  it('is false when the position went backwards (a seek before the first picture)', () => {
    expect(videoHasPainted(5_000, 1_000)).toBe(false);
  });
});
