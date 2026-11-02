import {
  Bookmark,
  HomeSection,
  MediaDetails,
  PlaybackStatus,
  ProviderInfo,
  RepositoryEntry,
  SearchItem,
  WatchProgress,
} from '../types';

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

/** A readable message for anything that was thrown or rejected. */
export function describeError(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  return 'Something went wrong. See the log for details.';
}

/**
 * Calls a Rust command. Failures are never replaced by made-up data: the rejection reaches the
 * caller, which shows it to the user.
 */
async function invokeTauri<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  if (typeof window === 'undefined' || !window.__TAURI_INTERNALS__) {
    throw new Error('The CloudStream backend is not available (open the desktop app, not a browser).');
  }
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(cmd, args);
}

export const api = {
  getProviders: () => invokeTauri<ProviderInfo[]>('get_available_providers'),

  getHomeSections: (providerId?: string) =>
    invokeTauri<HomeSection[]>('get_home_sections', { providerId }),

  search: (query: string, providerId?: string) =>
    invokeTauri<SearchItem[]>('search_media', { query, providerId }),

  getDetails: (providerId: string, url: string) =>
    invokeTauri<MediaDetails>('get_media_details', { providerId, url }),

  playMedia: (params: {
    provider: string;
    media_key: string;
    title: string;
    detail_url?: string;
    poster_url?: string;
  }) => invokeTauri<void>('play_media', { request: params }),

  pause: () => invokeTauri<void>('pause_playback'),

  resume: () => invokeTauri<void>('resume_playback'),

  seekTo: (positionMs: number) => invokeTauri<void>('seek_to', { positionMs }),

  stop: () => invokeTauri<void>('stop_playback'),

  setVideoSurface: (width: number, height: number) =>
    invokeTauri<void>('set_video_surface', { width, height }),

  /** The newest frame (16-byte header + RGBA), or an empty buffer when nothing is newer. */
  getVideoFrame: (lastSerial: number) => invokeTauri<ArrayBuffer>('get_video_frame', { lastSerial }),

  /** Polled several times a second; a failed poll just reports an idle player. */
  async getStatus(): Promise<PlaybackStatus> {
    try {
      return await invokeTauri<PlaybackStatus>('get_playback_status');
    } catch {
      return { state: 'Idle', position_ms: 0, duration_ms: 0, has_video: false, volume: 100, tracks: [], error: null };
    }
  },

  selectTrack: (kind: 'audio' | 'subtitle', id: number | null) =>
    invokeTauri<void>('select_media_track', { kind, id }),

  getBookmarks: () => invokeTauri<Bookmark[]>('get_bookmarks'),

  addBookmark: (bookmark: Bookmark) => invokeTauri<void>('add_bookmark', { payload: bookmark }),

  removeBookmark: (provider: string, url: string) =>
    invokeTauri<void>('remove_bookmark', { provider, url }),

  isBookmarked: (provider: string, url: string) =>
    invokeTauri<boolean>('is_bookmarked', { provider, url }),

  getContinueWatching: () => invokeTauri<WatchProgress[]>('get_continue_watching'),

  getRepositories: () => invokeTauri<RepositoryEntry[]>('get_repositories'),

  addRepository: (url: string) => invokeTauri<void>('add_repository', { url }),

  removeRepository: (url: string) => invokeTauri<void>('remove_repository', { url }),
};
