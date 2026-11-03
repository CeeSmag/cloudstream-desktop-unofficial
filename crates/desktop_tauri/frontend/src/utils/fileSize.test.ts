import { describe, expect, it } from 'vitest';
import { formatFileSize } from './fileSize';

describe('formatFileSize', () => {
  it('shows a dash for a file that does not exist', () => {
    expect(formatFileSize(null)).toBe('—');
  });

  it('uses bytes, kilobytes and megabytes', () => {
    expect(formatFileSize(0)).toBe('0 B');
    expect(formatFileSize(1536)).toBe('1.5 KB');
    expect(formatFileSize(5 * 1024 * 1024)).toBe('5.0 MB');
  });
});
