export interface ProviderInfo {
  name: string;
  mainUrl: string;
  lang: string;
  types: string[];
  hasMainPage: boolean;
  mainPage: {
    name: string;
    data: string;
    horizontalImages: boolean;
  }[];
}

export interface HomeItem {
  id: string;
  name: string;
  url: string;
  poster_url: string | null;
  provider: string;
  media_type: string | null;
  year: number | null;
}

export interface HomeSection {
  title: string;
  items: HomeItem[];
  is_horizontal: boolean;
}

export interface SearchItem {
  id: string;
  name: string;
  url: string;
  poster_url: string | null;
  provider: string;
  media_type?: string | null;
  year: number | null;
}

export interface Episode {
  id: string;
  data: string;
  name: string;
  season: number | null;
  episode: number | null;
  poster_url: string | null;
  description: string | null;
}

export interface MediaDetails {
  name: string;
  url: string;
  provider: string;
  media_type: string | null;
  data_url: string | null;
  poster_url: string | null;
  year: number | null;
  plot: string | null;
  tags: string[];
  episodes: Episode[];
  /** Wide background picture. */
  background_url: string | null;
  /** Rating out of 10. */
  rating: number | null;
  duration_minutes: number | null;
  cast: CastMember[];
  /** Ids of the title in the skip-time databases, when the provider reports them. */
  imdb_id: string | null;
  tmdb_id: string | null;
  mal_id: string | null;
}

/** A stretch of the video that can be jumped over. */
export interface SkipSegment {
  kind: 'intro' | 'recap' | 'credits' | 'preview';
  start_ms: number;
  end_ms: number;
}

export interface CastMember {
  name: string;
  image: string | null;
  role: string | null;
}

export interface Bookmark {
  id: string;
  title: string;
  poster_url: string | null;
  status: string;
  provider: string;
  url: string;
}

export interface WatchProgress {
  /** Provider payload identifying the playable item (episode data or movie URL). */
  media_id: string;
  provider: string;
  /** Provider URL of the parent title; empty when it was never recorded. */
  detail_url: string;
  poster_url: string | null;
  title: string;
  position_ms: number;
  duration_ms: number;
  percentage: number;
  updated_at: number;
  season?: number | null;
  episode?: number | null;
}

export interface NextEpisode {
  media_key: string;
  season: number | null;
  episode: number | null;
}

export interface ResumePointer {
  provider: string;
  detail_url: string;
  media_key: string;
  title: string;
  poster_url: string | null;
  season: number | null;
  episode: number | null;
  position_ms: number;
  duration_ms: number;
}

export interface MediaTrack {
  id: number;
  kind: 'audio' | 'subtitle';
  title: string;
  language: string | null;
  selected: boolean;
}

/** A named stretch of the video that starts at `time_ms`. */
export interface Chapter {
  title: string;
  time_ms: number;
}

export interface PlaybackStatus {
  state: 'Idle' | 'Loading' | 'Playing' | 'Paused' | 'Ended' | 'Failed';
  position_ms: number;
  duration_ms: number;
  has_video: boolean;
  volume: number;
  tracks: MediaTrack[];
  /** Chapters of the stream in time order; empty when it has none. */
  chapters: Chapter[];
  error?: string | null;
}

export interface RepositoryEntry {
  url: string;
  name: string;
  addedAtMs: number;
  /** Address of the repository's icon, when it has one. */
  iconUrl: string | null;
}

/** One stream the player can switch to. */
export interface SourceInfo {
  index: number;
  label: string;
  selected: boolean;
  /** True while the check of this stream has not finished. */
  checking: boolean;
  /** `HLS`, `DASH`, a file type such as `MKV`, or `Video`. */
  kind: string;
  /** Host the stream is served from. */
  host: string;
  /** The stream address. */
  url: string;
  /** File size in bytes, null until the check learns it (and always for playlists). */
  size_bytes: number | null;
  /** Languages of the subtitles this source offers, as named (`eng`, `Arabic`); known once it is checked. */
  subtitle_languages: string[];
}

/** An extension that is installed on this computer. */
export interface InstalledExtension {
  /** Address of its icon, when its repository listed one. */
  icon_url: string | null;
  internal_name: string;
  version: number;
  repository_url: string | null;
  trusted: boolean;
  providers: string[];
}

/** A plugin a repository offers. */
export interface RepositoryPlugin {
  internal_name: string;
  name: string;
  version: number;
  description: string | null;
  authors: string[];
  language: string | null;
  icon_url: string | null;
  file_size: number | null;
  installed_version: number | null;
}

/** An extension that could not be loaded at start-up. */
export interface StartupFailure {
  name: string;
  reason: string;
}

/** How far the app's start-up (loading the installed extensions) has come. */
/** A site that needs Cloudflare's check before it can load, and the viewer's go-ahead to open the check window. */
export interface CloudflareNeeded {
  host: string;
  provider: string | null;
}

export interface StartupStatus {
  ready: boolean;
  finished: number;
  total: number;
  current: string;
  failures: StartupFailure[];
}

/** How far the search and check of a video's streams have come; `total` grows while streams are found. */
export interface CheckProgress {
  checked: number;
  total: number;
  working: number;
  /** The viewer asked to start without waiting for the rest. */
  skipping: boolean;
  /** The provider is still looking for streams. */
  discovering: boolean;
  /** What is going on with sources saved when the video was opened before. */
  saved: SavedSources;
}

/** `trusted`: playing from sources saved a short while ago; `testing`: trying the best of older ones;
 * `outOfDate`: none of them works any more, so the provider is searched again. */
export type SavedSources = 'none' | 'trusted' | 'testing' | 'outOfDate';

/** A debug log file. */
export interface LogFileInfo {
  label: string;
  file_name: string;
  /** Null while the file does not exist. */
  size_bytes: number | null;
}

/** State of debug logging: whether it is on, where the files are and how big they are. */
export interface DebugLoggingInfo {
  enabled: boolean;
  folder: string;
  files: LogFileInfo[];
}

/** A language that can be chosen as the preferred audio language. */
export interface LanguageOption {
  code: string;
  name: string;
  /** Every code that stands for the language, such as `eng` and `en`. */
  codes: string[];
}

/** The viewer's preferences and the choices on offer. */
export interface Preferences {
  /** Code of the preferred audio language; null when the stream's own default track is wanted. */
  preferred_audio: string | null;
  /** Codes of the languages whose subtitles get a flag in the player, in order (at most three). */
  preferred_subtitles: string[];
  /** Whether the subtitle in the first of those languages the video has turns on by itself. */
  auto_subtitles: boolean;
  /** Whether the next episode starts by itself when one ends. */
  autoplay_next: boolean;
  /** Whether skip times are looked up online by the title's id. */
  skip_lookup: boolean;
  languages: LanguageOption[];
}

/** An extension found by searching all repositories, with the repository that offers it. */
export interface PluginHit {
  repository_url: string;
  repository_name: string;
  repository_icon_url: string | null;
  plugin: RepositoryPlugin;
}

/** One media card item shown on the Browse page. */
export interface BrowseItem {
  id: string;
  name: string;
  poster_url: string | null;
  year: number | null;
  rating: number | null;
  media_type: string;
}

/** Filter parameters for the Browse page. */
export interface BrowseFilters {
  media_type: 'movie' | 'tv';
  year: number | null;
  sort_by: 'popular' | 'rating';
  rating: number | null;
  genres: string[];
  page: number;
}

/** Paginated result returned from Browse catalog query. */
export interface BrowsePageResult {
  items: BrowseItem[];
  page: number;
  total_pages: number;
}

/** Result of checking GitHub for application updates. */
export interface UpdateCheckResult {
  current_version: string;
  latest_version: string;
  has_update: boolean;
  release_name: string | null;
  release_notes: string | null;
  download_url: string | null;
  asset_name: string | null;
  asset_size: number | null;
  is_portable: boolean;
}

/** Download progress payload emitted during update installation. */
export interface UpdateProgressPayload {
  downloaded_bytes: number;
  total_bytes: number;
  percent: number;
}

/** The data folder's size limit and how much of it is used. */
export interface StorageInfo {
  cap_gb: number;
  used_bytes: number;
  over: boolean;
}
