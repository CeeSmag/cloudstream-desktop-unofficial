/** Why the Home page has nothing to show, when it is because of the plugins. */
export type HomeEmptyState = 'none' | 'no-plugins' | 'plugins-unavailable';

interface HomeEmptyInput {
  /** The Home feed is still being fetched. */
  loading: boolean;
  /** Providers the plugins offer right now. */
  providerCount: number;
  /** Plugins installed on this PC; null until that is known. */
  installedCount: number | null;
}

/**
 * Decides whether Home should ask the viewer to add plugins.
 * 'no-plugins' is a fresh start; 'plugins-unavailable' is plugins that are installed but not usable
 * (switched off, or failed to load), where adding more would not help.
 */
export function homeEmptyState({ loading, providerCount, installedCount }: HomeEmptyInput): HomeEmptyState {
  if (loading || installedCount === null || providerCount > 0) return 'none';
  return installedCount === 0 ? 'no-plugins' : 'plugins-unavailable';
}
