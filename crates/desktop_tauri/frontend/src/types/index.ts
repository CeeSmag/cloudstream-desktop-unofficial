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
  media_id: string;
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
  added_at_ms: number;
}
