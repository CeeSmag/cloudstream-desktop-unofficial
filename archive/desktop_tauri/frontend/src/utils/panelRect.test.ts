import { describe, expect, it } from 'vitest';
import { isUsableRect, rectOf, sameRect } from './panelRect';

describe('rectOf', () => {
  it('rounds to whole pixels', () => {
    expect(rectOf({ left: 10.4, top: 20.6, width: 599.5, height: 400.2 })).toEqual([10, 21, 600, 400]);
  });
});

describe('sameRect', () => {
  it('compares every number', () => {
    expect(sameRect([1, 2, 3, 4], [1, 2, 3, 4])).toBe(true);
    expect(sameRect([1, 2, 3, 4], [1, 2, 3, 5])).toBe(false);
  });

  it('treats nothing as equal only to nothing', () => {
    expect(sameRect(null, null)).toBe(true);
    expect(sameRect(null, [1, 2, 3, 4])).toBe(false);
  });
});

describe('isUsableRect', () => {
  it('refuses an empty or non-numeric frame', () => {
    expect(isUsableRect([0, 0, 600, 400])).toBe(true);
    expect(isUsableRect([0, 0, 0, 400])).toBe(false);
    expect(isUsableRect([0, 0, 600, 0.4])).toBe(false);
    expect(isUsableRect([Number.NaN, 0, 600, 400])).toBe(false);
  });
});
