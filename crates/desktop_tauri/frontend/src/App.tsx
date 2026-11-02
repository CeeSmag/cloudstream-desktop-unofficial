import React from 'react';
import { listen } from '@tauri-apps/api/event';
import { api, describeError } from './services/api';
import { ErrorToast } from './components/ErrorToast';
import {
  Bookmark,
  Episode,
  HomeItem,
  HomeSection,
  MediaDetails,
  PlaybackStatus,
  ProviderInfo,
  RepositoryEntry,
  WatchProgress,
} from './types';
import { Navigation } from './components/Navigation';
import { HomeScreen } from './screens/HomeScreen';
import { SearchScreen } from './screens/SearchScreen';
import { DetailsScreen } from './screens/DetailsScreen';
import { PlayerOverlay } from './screens/PlayerOverlay';
import { LibraryScreen } from './screens/LibraryScreen';
import { SettingsScreen } from './screens/SettingsScreen';
import { findEpisodeForKey, movieAsEpisode, playbackTitle } from './playback/episodeLookup';

export const App: React.FC = () => {
  const [currentTab, setCurrentTab] = React.useState('home');
  const [errorText, setErrorText] = React.useState<string | null>(null);

  // Runs an async action and shows its failure instead of dropping it.
  const guard = React.useCallback(
    <A extends unknown[]>(action: (...args: A) => Promise<void>) =>
      async (...args: A): Promise<void> => {
        try {
          await action(...args);
        } catch (error) {
          setErrorText(describeError(error));
        }
      },
    [],
  );
  const [providers, setProviders] = React.useState<ProviderInfo[]>([]);
  const [selectedProvider, setSelectedProvider] = React.useState('');
  const [homeSections, setHomeSections] = React.useState<HomeSection[]>([]);
  const [homeLoading, setHomeLoading] = React.useState(true);

  // Navigation states
  const [activeDetails, setActiveDetails] = React.useState<MediaDetails | null>(null);
  const [detailsLoading, setDetailsLoading] = React.useState(false);

  // Player overlay states
  const [activeEpisode, setActiveEpisode] = React.useState<Episode | null>(null);
  const [activePlayerTitle, setActivePlayerTitle] = React.useState('');
  const [playerStatus, setPlayerStatus] = React.useState<PlaybackStatus>({
    state: 'Idle',
    position_ms: 0,
    duration_ms: 0,
    has_video: false,
    volume: 100,
    tracks: [],
  });

  // Library & settings
  const [bookmarks, setBookmarks] = React.useState<Bookmark[]>([]);
  const [continueWatching, setContinueWatching] = React.useState<WatchProgress[]>([]);
  const [repositories, setRepositories] = React.useState<RepositoryEntry[]>([]);
  const [isCurrentBookmarked, setIsCurrentBookmarked] = React.useState(false);

  // Loads providers and the first provider's home page.
  const refreshProviders = React.useCallback(async () => {
    const provs = await api.getProviders();
    setProviders(provs);
    const first = provs[0]?.name || '';
    setSelectedProvider((current) => (provs.some((p) => p.name === current) ? current : first));
    const sections = await api.getHomeSections(first || undefined);
    setHomeSections(sections);
  }, []);

  // Initial load
  React.useEffect(() => {
    async function init() {
      try {
        await refreshProviders();
        const [bms, cw, repos] = await Promise.all([
          api.getBookmarks(),
          api.getContinueWatching(),
          api.getRepositories(),
        ]);
        setBookmarks(bms);
        setContinueWatching(cw);
        setRepositories(repos);
      } finally {
        setHomeLoading(false);
      }
    }
    init();
  }, [refreshProviders]);

  // Extensions finish loading in the background; reload providers when the backend says so.
  React.useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen('providers-changed', () => {
      setHomeLoading(true);
      refreshProviders().finally(() => setHomeLoading(false));
    })
      .then((stop) => {
        unlisten = stop;
      })
      .catch(() => undefined);
    return () => unlisten?.();
  }, [refreshProviders]);

  // Switch provider
  const handleSelectProvider = guard(async (p: string) => {
    setSelectedProvider(p);
    setHomeLoading(true);
    try {
      const sections = await api.getHomeSections(p);
      setHomeSections(sections);
    } finally {
      setHomeLoading(false);
    }
  });

  // Open Details Screen
  const handleSelectItem = guard(async (item: HomeItem) => {
    setDetailsLoading(true);
    try {
      const details = await api.getDetails(item.provider, item.url);
      setActiveDetails(details);
      const bookmarked = await api.isBookmarked(details.provider, details.url);
      setIsCurrentBookmarked(bookmarked);
    } finally {
      setDetailsLoading(false);
    }
  });

  // Start Playback
  // Plays one episode (or a movie's single stream) of `details`; the title page is recorded so
  // the entry can be resumed later from Continue Watching.
  const startEpisode = async (details: MediaDetails, ep: Episode) => {
    const title = playbackTitle(details.name, ep.name);
    await api.playMedia({
      provider: details.provider,
      media_key: ep.data,
      title,
      detail_url: details.url,
      poster_url: ep.poster_url || details.poster_url || undefined,
    });
    setActivePlayerTitle(title);
    setActiveEpisode(ep);
  };

  const handlePlayEpisode = guard(async (ep: Episode) => {
    if (!activeDetails) throw new Error('Open a title before playing an episode.');
    await startEpisode(activeDetails, ep);
  });

  const handlePlayItemDirect = guard(async (item: HomeItem) => {
    const details = await api.getDetails(item.provider, item.url);
    const target = details.episodes[0] ?? movieAsEpisode(details);
    if (!target) {
      throw new Error('This title has nothing to play.');
    }
    await startEpisode(details, target);
  });

  const handleResume = guard(async (progress: WatchProgress) => {
    if (!progress.detail_url) {
      throw new Error(
        `"${progress.title}" was saved without its title page. Open it from Search or Home and play it once to make it resumable.`,
      );
    }
    const details = await api.getDetails(progress.provider, progress.detail_url);
    const target = findEpisodeForKey(details, progress.media_id);
    if (!target) {
      throw new Error(`"${progress.title}" is no longer listed by ${progress.provider}.`);
    }
    await startEpisode(details, target);
  });

  // Player polling tick
  React.useEffect(() => {
    if (!activeEpisode) return;
    const interval = setInterval(async () => {
      const st = await api.getStatus();
      setPlayerStatus(st);
    }, 800);
    return () => clearInterval(interval);
  }, [activeEpisode]);

  // Stop player & return
  const handleStopPlayer = guard(async () => {
    await api.stop();
    setActiveEpisode(null);
    // Refresh continue watching
    const cw = await api.getContinueWatching();
    setContinueWatching(cw);
  });

  // Toggle bookmark
  const handleToggleBookmark = guard(async () => {
    if (!activeDetails) return;
    if (isCurrentBookmarked) {
      await api.removeBookmark(activeDetails.provider, activeDetails.url);
      setIsCurrentBookmarked(false);
    } else {
      await api.addBookmark({
        id: activeDetails.url,
        title: activeDetails.name,
        poster_url: activeDetails.poster_url,
        status: 'Watching',
        provider: activeDetails.provider,
        url: activeDetails.url,
      });
      setIsCurrentBookmarked(true);
    }
    const bms = await api.getBookmarks();
    setBookmarks(bms);
  });

  const handleRemoveBookmark = guard(async (provider: string, url: string) => {
    await api.removeBookmark(provider, url);
    const bms = await api.getBookmarks();
    setBookmarks(bms);
  });

  // If player is active, render 100% transparent overlay over mpv
  if (activeEpisode) {
    return (
      <>
        <ErrorToast message={errorText} onDismiss={() => setErrorText(null)} />
        <PlayerOverlay
        title={activePlayerTitle}
        status={playerStatus}
        onBack={handleStopPlayer}
        onTogglePlay={guard(async () => {
          if (playerStatus.state === 'Playing') {
            await api.pause();
          } else {
            await api.resume();
          }
        })}
        onSeek={guard(async (pos: number) => {
          await api.seekTo(pos);
        })}
        onSelectTrack={guard(async (kind: 'audio' | 'subtitle', id: number | null) => {
          await api.selectTrack(kind, id);
        })}
      />
      </>
    );
  }

  return (
    <div className="flex flex-col h-screen w-screen bg-midnight text-white overflow-hidden">
      <ErrorToast message={errorText} onDismiss={() => setErrorText(null)} />
      {/* Top Navigation */}
      <Navigation
        currentTab={currentTab}
        onSelectTab={(tab) => {
          setActiveDetails(null);
          setCurrentTab(tab);
        }}
        providers={providers}
        selectedProvider={selectedProvider}
        onSelectProvider={handleSelectProvider}
      />

      {/* Main View Area */}
      <main className="flex-1 flex overflow-hidden">
        {detailsLoading ? (
          <div className="flex-1 flex items-center justify-center text-zinc-500">
            <div className="w-8 h-8 rounded-full border-2 border-accent border-t-transparent animate-spin mb-4" />
            <span className="ml-3 text-sm">Loading details...</span>
          </div>
        ) : activeDetails ? (
          <DetailsScreen
            details={activeDetails}
            onBack={() => setActiveDetails(null)}
            onPlayEpisode={handlePlayEpisode}
            isBookmarked={isCurrentBookmarked}
            onToggleBookmark={handleToggleBookmark}
          />
        ) : (
          <>
            {currentTab === 'home' && (
              <HomeScreen
                sections={homeSections}
                onSelectItem={handleSelectItem}
                onPlayItem={handlePlayItemDirect}
                loading={homeLoading}
              />
            )}
            {currentTab === 'search' && (
              <SearchScreen
                onSelectItem={handleSelectItem}
                onPlayItem={handlePlayItemDirect}
                selectedProvider={selectedProvider}
                onSearch={(q) => api.search(q, selectedProvider || undefined)}
              />
            )}
            {currentTab === 'library' && (
              <LibraryScreen
                bookmarks={bookmarks}
                continueWatching={continueWatching}
                onSelectItem={handleSelectItem}
                onPlayItem={handlePlayItemDirect}
                onResume={handleResume}
                onRemoveBookmark={handleRemoveBookmark}
              />
            )}
            {currentTab === 'settings' && (
              <SettingsScreen
                repositories={repositories}
                providers={providers}
                onAddRepo={guard(async (url: string) => {
                  await api.addRepository(url);
                  const repos = await api.getRepositories();
                  setRepositories(repos);
                })}
                onRemoveRepo={guard(async (url: string) => {
                  await api.removeRepository(url);
                  const repos = await api.getRepositories();
                  setRepositories(repos);
                })}
              />
            )}
          </>
        )}
      </main>
    </div>
  );
};
export default App;
