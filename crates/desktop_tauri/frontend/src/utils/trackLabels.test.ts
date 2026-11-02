import { describe, expect, it } from 'vitest';
import { numberDuplicateLabels } from './trackLabels';

describe('numberDuplicateLabels', () => {
  it('leaves unique labels alone', () => {
    expect(numberDuplicateLabels(['English', 'Hindi'])).toEqual(['English', 'Hindi']);
  });

  it('numbers every entry of a repeated label, in order', () => {
    expect(numberDuplicateLabels(['English', 'Arabic', 'English', 'English'])).toEqual([
      'English 1',
      'Arabic',
      'English 2',
      'English 3',
    ]);
  });

  it('handles an empty list', () => {
    expect(numberDuplicateLabels([])).toEqual([]);
  });
});
