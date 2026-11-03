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
}

export interface MediaTrack {
  id: number;
  kind: 'audio' | 'subtitle';
  title: string;
  language: string | null;
  selected: boolean;
}

export interface PlaybackStatus {
  state: 'Idle' | 'Loading' | 'Playing' | 'Paused' | 'Ended' | 'Failed';
  position_ms: number;
  duration_ms: number;
  has_video: boolean;
  volume: number;
  tracks: MediaTrack[];
  error?: string | null;
}

export interface RepositoryEntry {
  url: string;
  name: string;
  addedAtMs: number;
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
}

/** An extension that is installed on this computer. */
export interface InstalledExtension {
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
export interface StartupStatus {
  ready: boolean;
  finished: number;
  total: number;
  current: string;
  failures: StartupFailure[];
}

/** How far the check of a video's streams has come; `total` is 0 while the streams are still being looked up. */
export interface CheckProgress {
  checked: number;
  total: number;
  working: number;
  /** The viewer asked to start without waiting for the rest. */
  skipping: boolean;
}

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
