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

async function invokeTauri<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  if (typeof window !== 'undefined' && window.__TAURI_INTERNALS__) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(cmd, args);
  }
  return Promise.reject(new Error(`Tauri invoke unavailable for ${cmd}`));
}

export const api = {
  async getProviders(): Promise<ProviderInfo[]> {
    try {
      return await invokeTauri<ProviderInfo[]>('get_available_providers');
    } catch {
      return [
        {
          name: 'SuperStream',
          mainUrl: 'https://superstream.media',
          lang: 'en',
          types: ['Movie', 'TvSeries'],
          hasMainPage: true,
          mainPage: [
            { name: 'Trending Movies', data: 'trending-movies', horizontalImages: false },
            { name: 'Popular TV Shows', data: 'popular-shows', horizontalImages: false },
          ],
        },
        {
          name: 'SoraStream',
          mainUrl: 'https://sorastream.app',
          lang: 'en',
          types: ['Movie', 'Anime'],
          hasMainPage: true,
          mainPage: [
            { name: 'Featured Anime', data: 'featured-anime', horizontalImages: true },
          ],
        },
      ];
    }
  },

  async getHomeSections(providerId?: string): Promise<HomeSection[]> {
    try {
      return await invokeTauri<HomeSection[]>('get_home_sections', { providerId });
    } catch {
      return [
        {
          title: 'Trending Movies',
          is_horizontal: false,
          items: [
            {
              id: '1',
              name: 'Dune: Part Two',
              url: 'https://example.com/dune2',
              poster_url: 'https://image.tmdb.org/t/p/w500/8b8R8l88Qje9dn9OE8PY05Nxl1X.jpg',
              provider: 'SuperStream',
              media_type: 'Movie',
              year: 2024,
            },
            {
              id: '2',
              name: 'Oppenheimer',
              url: 'https://example.com/oppenheimer',
              poster_url: 'https://image.tmdb.org/t/p/w500/ptpr0kGAckfQkJeJIt8st5dglvd.jpg',
              provider: 'SuperStream',
              media_type: 'Movie',
              year: 2023,
            },
            {
              id: '3',
              name: 'Spider-Man: Across the Spider-Verse',
              url: 'https://example.com/spiderman',
              poster_url: 'https://image.tmdb.org/t/p/w500/8Vt6mWEReuy4Of61Lnj5Xj704m8.jpg',
              provider: 'SuperStream',
              media_type: 'Movie',
              year: 2023,
            },
            {
              id: '4',
              name: 'Interstellar',
              url: 'https://example.com/interstellar',
              poster_url: 'https://image.tmdb.org/t/p/w500/gEU2QniE6E77NI6lCU6MxlNBvIx.jpg',
              provider: 'SuperStream',
              media_type: 'Movie',
              year: 2014,
            },
            {
              id: '5',
              name: 'Blade Runner 2049',
              url: 'https://example.com/bladerunner',
              poster_url: 'https://image.tmdb.org/t/p/w500/gajva2L0rPYkEWjzgFlBXCAVBE5.jpg',
              provider: 'SuperStream',
              media_type: 'Movie',
              year: 2017,
            },
          ],
        },
        {
          title: 'Popular TV Shows',
          is_horizontal: false,
          items: [
            {
              id: '6',
              name: 'Arcane',
              url: 'https://example.com/arcane',
              poster_url: 'https://image.tmdb.org/t/p/w500/fqldf2t8ztc9aiwn397rHumwNtL.jpg',
              provider: 'SuperStream',
              media_type: 'TvSeries',
              year: 2024,
            },
            {
              id: '7',
              name: 'Shōgun',
              url: 'https://example.com/shogun',
              poster_url: 'https://image.tmdb.org/t/p/w500/7O4iVfOMQmdCSxhOg1WnzG1AgYT.jpg',
              provider: 'SuperStream',
              media_type: 'TvSeries',
              year: 2024,
            },
            {
              id: '8',
              name: 'The Last of Us',
              url: 'https://example.com/tlou',
              poster_url: 'https://image.tmdb.org/t/p/w500/uKvVjK1qznIRINZdHHgZscqWFHG.jpg',
              provider: 'SuperStream',
              media_type: 'TvSeries',
              year: 2023,
            },
          ],
        },
      ];
    }
  },

  async search(query: string, providerId?: string): Promise<SearchItem[]> {
    try {
      return await invokeTauri<SearchItem[]>('search_media', { query, providerId });
    } catch {
      return [];
    }
  },

  async getDetails(providerId: string, url: string): Promise<MediaDetails> {
    try {
      return await invokeTauri<MediaDetails>('get_media_details', { providerId, url });
    } catch {
      return {
        name: 'Dune: Part Two',
        url,
        provider: providerId,
        media_type: 'Movie',
        data_url: 'https://example.com/dune2/stream',
        poster_url: 'https://image.tmdb.org/t/p/w500/8b8R8l88Qje9dn9OE8PY05Nxl1X.jpg',
        year: 2024,
        plot: 'Paul Atreides unites with Chani and the Fremen while seeking revenge against the conspirators who destroyed his family.',
        tags: ['Sci-Fi', 'Adventure', 'Action'],
        episodes: [
          {
            id: 'ep1',
            data: 'https://example.com/dune2/stream',
            name: 'Full Feature',
            season: 1,
            episode: 1,
            poster_url: 'https://image.tmdb.org/t/p/w500/8b8R8l88Qje9dn9OE8PY05Nxl1X.jpg',
            description: 'Theatrical Cut (166 min)',
          },
        ],
      };
    }
  },

  async playMedia(params: {
    provider: string;
    media_key: string;
    title: string;
    detail_url?: string;
    poster_url?: string;
  }): Promise<void> {
    try {
      await invokeTauri('play_media', { request: params });
    } catch (err) {
      console.warn('Playback error (mock mode):', err);
    }
  },

  async pause(): Promise<void> {
    await invokeTauri('pause_playback').catch(() => {});
  },

  async resume(): Promise<void> {
    await invokeTauri('resume_playback').catch(() => {});
  },

  async seekTo(positionMs: number): Promise<void> {
    await invokeTauri('seek_to', { positionMs }).catch(() => {});
  },

  async stop(): Promise<void> {
    await invokeTauri('stop_playback').catch(() => {});
  },

  async getStatus(): Promise<PlaybackStatus> {
    try {
      return await invokeTauri<PlaybackStatus>('get_playback_status');
    } catch {
      return {
        state: 'Idle',
        position_ms: 0,
        duration_ms: 0,
        has_video: false,
        volume: 100,
        tracks: [],
      };
    }
  },

  async selectTrack(kind: 'audio' | 'subtitle', id: number | null): Promise<void> {
    await invokeTauri('select_media_track', { kind, id }).catch(() => {});
  },

  async getBookmarks(): Promise<Bookmark[]> {
    try {
      return await invokeTauri<Bookmark[]>('get_bookmarks');
    } catch {
      return [];
    }
  },

  async addBookmark(bookmark: Bookmark): Promise<void> {
    await invokeTauri('add_bookmark', { payload: bookmark }).catch(() => {});
  },

  async removeBookmark(provider: string, url: string): Promise<void> {
    await invokeTauri('remove_bookmark', { provider, url }).catch(() => {});
  },

  async isBookmarked(provider: string, url: string): Promise<boolean> {
    try {
      return await invokeTauri<boolean>('is_bookmarked', { provider, url });
    } catch {
      return false;
    }
  },

  async getContinueWatching(): Promise<WatchProgress[]> {
    try {
      return await invokeTauri<WatchProgress[]>('get_continue_watching');
    } catch {
      return [];
    }
  },

  async getRepositories(): Promise<RepositoryEntry[]> {
    try {
      return await invokeTauri<RepositoryEntry[]>('get_repositories');
    } catch {
      return [];
    }
  },

  async addRepository(url: string): Promise<void> {
    await invokeTauri('add_repository', { url });
  },

  async removeRepository(url: string): Promise<void> {
    await invokeTauri('remove_repository', { url });
  },
};
