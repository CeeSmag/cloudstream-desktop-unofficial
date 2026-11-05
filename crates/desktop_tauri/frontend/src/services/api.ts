import {
  Bookmark,
  BrowseFilters,
  BrowsePageResult,
  HomeSection,
  DebugLoggingInfo,
  InstalledExtension,
  MediaDetails,
  NextEpisode,
  PlaybackStatus,
  PluginHit,
  Preferences,
  ProviderInfo,
  RepositoryEntry,
  RepositoryPlugin,
  ResumePointer,
  SearchItem,
  SourceInfo,
  StartupStatus,
  SkipSegment,
  WatchProgress,
} from '../types';
import { describeValue, isDebugLogging, logEvent } from '../utils/logger';
import type { SkipRequest } from '../playback/skipRequest';

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

/** Commands called many times a second; logging each call would only bury the rest. */
const SILENT_COMMANDS = ['get_video_frame', 'get_playback_status', 'log_from_webapp', 'get_debug_logging'];

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
  const started = performance.now();
  try {
    const result = await invoke<T>(cmd, args);
    if (isDebugLogging() && !SILENT_COMMANDS.includes(cmd)) {
      logEvent('debug', `${cmd} finished in ${Math.round(performance.now() - started)} ms`);
    }
    return result;
  } catch (error) {
    logEvent('warn', `${cmd} failed after ${Math.round(performance.now() - started)} ms: ${describeValue(error)}`);
    throw error;
  }
}

export const api = {
  getStartupStatus: () => invokeTauri<StartupStatus>('get_startup_status'),

  getPreferences: () => invokeTauri<Preferences>('get_preferences'),

  /** Saves the preferred audio language; null means the stream's own default track. */
  setPreferredAudio: (language: string | null) => invokeTauri<Preferences>('set_preferred_audio', { language }),

  /** Saves the languages whose subtitles get a flag in the player (at most three). */
  setPreferredSubtitles: (languages: string[]) => invokeTauri<Preferences>('set_preferred_subtitles', { languages }),

  /** Switches on or off turning on a preferred-language subtitle by itself. */
  setAutoSubtitles: (enabled: boolean) => invokeTauri<Preferences>('set_auto_subtitles', { enabled }),

  /** Switches `autoplay_next` or `skip_lookup` on or off. */
  setPlaybackOption: (option: 'autoplay_next' | 'skip_lookup', enabled: boolean) =>
    invokeTauri<Preferences>('set_playback_option', { option, enabled }),

  getDebugLogging: () => invokeTauri<DebugLoggingInfo>('get_debug_logging'),

  setDebugLogging: (enabled: boolean) => invokeTauri<DebugLoggingInfo>('set_debug_logging', { enabled }),

  openLogsFolder: () => invokeTauri<void>('open_logs_folder'),

  getProviders: () => invokeTauri<ProviderInfo[]>('get_available_providers'),

  getHomeSections: (providerId?: string) =>
    invokeTauri<HomeSection[]>('get_home_sections', { providerId }),

  search: (query: string, providerId?: string) =>
    invokeTauri<SearchItem[]>('search_media', { query, providerId }),

  browseCatalog: (filters: BrowseFilters) =>
    invokeTauri<BrowsePageResult>('browse_catalog', { request: filters }),

  getDetails: (providerId: string, url: string) =>
    invokeTauri<MediaDetails>('get_media_details', { providerId, url }),

  playMedia: (params: {
    provider: string;
    media_key: string;
    title: string;
    detail_url?: string;
    poster_url?: string;
    season?: number | null;
    episode?: number | null;
    next_episode?: NextEpisode | null;
  }) => invokeTauri<void>('play_media', { request: params }),

  /** Starts the video now on the best stream already known to work; the checks go on in the background. */
  skipSourceCheck: () => invokeTauri<void>('skip_source_check'),

  pause: () => invokeTauri<void>('pause_playback'),

  resume: () => invokeTauri<void>('resume_playback'),

  seekTo: (positionMs: number) => invokeTauri<void>('seek_to', { positionMs }),

  stop: () => invokeTauri<void>('stop_playback'),

  setVolume: (percent: number) => invokeTauri<void>('set_volume', { percent }),

  getSources: () => invokeTauri<SourceInfo[]>('get_playback_sources'),

  switchSource: (index: number) => invokeTauri<void>('switch_source', { index }),

  /** Puts the application window into or out of native fullscreen. */
  setWindowFullscreen: (fullscreen: boolean) => invokeTauri<void>('set_window_fullscreen', { fullscreen }),

  setVideoSurface: (width: number, height: number) =>
    invokeTauri<void>('set_video_surface', { width, height }),

  /** The newest frame (16-byte header + RGBA), or an empty buffer when nothing is newer. */
  getVideoFrame: (lastSerial: number) => invokeTauri<ArrayBuffer>('get_video_frame', { lastSerial }),

  /** Polled several times a second; a failed poll just reports an idle player. */
  async getStatus(): Promise<PlaybackStatus> {
    try {
      return await invokeTauri<PlaybackStatus>('get_playback_status');
    } catch {
      return { state: 'Idle', position_ms: 0, duration_ms: 0, has_video: false, volume: 100, tracks: [], chapters: [], error: null };
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

  /** Tells the backend the viewer picked another provider, so saved stream links are not used any more. */
  providerSwitched: (from: string, to: string) => invokeTauri<boolean>('provider_switched', { from, to }),

  /** The intro, recap and credits of the playing video; empty when none are known. */
  getSkipSegments: (request: SkipRequest) => invokeTauri<SkipSegment[]>('get_skip_segments', { request }),

  /** The saved position of every episode of a show, for the marks on its title page. */
  getShowProgress: (provider: string, detailUrl: string) =>
    invokeTauri<WatchProgress[]>('get_show_progress', { provider, detailUrl }),

  /** Reads the resume pointer for a show or movie (what episode to resume and remaining time). */
  getResumePointer: (provider: string, detailUrl: string) =>
    invokeTauri<ResumePointer | null>('get_resume_pointer', { provider, detailUrl }),

  /** Takes one title out of Continue Watching and forgets what was kept on disk for it. */
  removeContinueWatching: (provider: string, detailUrl: string) =>
    invokeTauri<void>('remove_continue_watching', { provider, detailUrl }),

  /** Empties Continue Watching and forgets what was kept on disk for its titles. */
  clearContinueWatching: () => invokeTauri<void>('clear_continue_watching'),

  getRepositories: () => invokeTauri<RepositoryEntry[]>('get_repositories'),

  addRepository: (url: string) => invokeTauri<void>('add_repository', { url }),

  removeRepository: (url: string) => invokeTauri<void>('remove_repository', { url }),

  getInstalledExtensions: () => invokeTauri<InstalledExtension[]>('get_installed_extensions'),

  /** Looks up the icons of repositories that have none yet; returns how many were looked at. */
  refreshRepositoryIcons: () => invokeTauri<number>('refresh_repository_icons'),

  /** Downloads the extensions of every repository so they can be listed; returns how many there are. */
  prepareExtensionCatalog: () => invokeTauri<number>('prepare_plugin_catalog'),

  /** Every extension of every repository, with the repository that offers it. */
  listAllPlugins: () => invokeTauri<PluginHit[]>('list_all_plugins'),

  getRepositoryPlugins: (url: string) => invokeTauri<RepositoryPlugin[]>('get_repository_plugins', { url }),

  installPlugin: (repositoryUrl: string, internalName: string) =>
    invokeTauri<string[]>('install_plugin', { repositoryUrl, internalName }),

  uninstallExtension: (internalName: string) => invokeTauri<void>('uninstall_extension', { internalName }),

  setExtensionTrusted: (internalName: string, trusted: boolean) =>
    invokeTauri<void>('set_extension_trusted', { internalName, trusted }),
};
