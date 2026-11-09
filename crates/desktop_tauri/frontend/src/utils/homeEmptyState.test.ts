import { describe, expect, it } from 'vitest';
import { homeEmptyState } from './homeEmptyState';

describe('homeEmptyState', () => {
  it('asks for plugins on a fresh start: nothing installed, nothing loading', () => {
    expect(homeEmptyState({ loading: false, providerCount: 0, installedCount: 0 })).toBe('no-plugins');
  });

  it('says nothing while the feed is loading or before the installed count is known', () => {
    expect(homeEmptyState({ loading: true, providerCount: 0, installedCount: 0 })).toBe('none');
    expect(homeEmptyState({ loading: false, providerCount: 0, installedCount: null })).toBe('none');
  });

  it('says nothing once a provider is available', () => {
    expect(homeEmptyState({ loading: false, providerCount: 2, installedCount: 3 })).toBe('none');
  });

  it('tells apart plugins that are installed but give no provider', () => {
    expect(homeEmptyState({ loading: false, providerCount: 0, installedCount: 2 })).toBe('plugins-unavailable');
  });
});
