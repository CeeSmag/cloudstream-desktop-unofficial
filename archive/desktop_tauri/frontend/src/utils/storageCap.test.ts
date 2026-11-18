import { describe, expect, it } from 'vitest';
import {
  capBytes,
  capLabel,
  DEFAULT_STORAGE_CAP_GB,
  STORAGE_CAP_OPTIONS,
  storageBreakdown,
  usedPercent,
} from './storageCap';
import type { StorageInfo } from '../types';

describe('storage limit options', () => {
  it('offers 1 to 10 GB and the default is one of them', () => {
    expect(STORAGE_CAP_OPTIONS[0]).toBe(1);
    expect(STORAGE_CAP_OPTIONS[STORAGE_CAP_OPTIONS.length - 1]).toBe(10);
    expect(STORAGE_CAP_OPTIONS).toHaveLength(10);
    expect(STORAGE_CAP_OPTIONS).toContain(DEFAULT_STORAGE_CAP_GB);
    expect(DEFAULT_STORAGE_CAP_GB).toBe(2);
  });

  it('counts in binary gigabytes, as the backend does', () => {
    expect(capBytes(1)).toBe(1_073_741_824);
    expect(capLabel(2)).toBe('2 GB');
  });
});

describe('usedPercent', () => {
  it('shows the used share and stops at 100', () => {
    expect(usedPercent(capBytes(2) / 2, 2)).toBe(50);
    expect(usedPercent(capBytes(2) * 3, 2)).toBe(100);
    expect(usedPercent(0, 2)).toBe(0);
  });
});

describe('storageBreakdown', () => {
  const base: StorageInfo = {
    cap_gb: 2,
    used_bytes: 100,
    over: false,
    logs_bytes: 10,
    subtitles_bytes: 0,
    sources_bytes: 20,
    updates_bytes: 0,
  };

  it('lists only folders that use space, in a fixed order', () => {
    expect(storageBreakdown(base)).toEqual([
      { label: 'Logs', bytes: 10 },
      { label: 'Saved sources', bytes: 20 },
    ]);
  });

  it('is empty when every tracked folder is empty', () => {
    expect(
      storageBreakdown({
        ...base,
        logs_bytes: 0,
        sources_bytes: 0,
      }),
    ).toEqual([]);
  });
});
