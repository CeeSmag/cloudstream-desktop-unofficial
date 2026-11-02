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
  ProviderInfo,
  WatchProgress,
} from './types';
import { Navigation } from './components/Navigation';
import { HomeScreen } from './screens/HomeScreen';
import { SearchScreen } from './screens/SearchScreen';
import { DetailsScreen } from './screens/DetailsScreen';
import { PlayerOverlay } from './screens/PlayerOverlay';
import { LibraryScreen } from './screens/LibraryScreen';
import { SettingsScreen } from './screens/SettingsScreen';
import { findEpisodeForKey, movieAsEpisode } from './playback/episodeLookup';
import { usePlayback } from './hooks/usePlayback';

export const App: React.FC = () => {
  const [currentTab, setCurrentTab] = React.useState('home');
  const [errorText, setErrorText] = React.useState<string | null>(null);
  const [searchQuery, setSearchQuery] = React.useState('');

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

  const playback = usePlayback();

  // Library & settings
  const [bookmarks, setBookmarks] = React.useState<Bookmark[]>([]);
  const [continueWatching, setContinueWatching] = React.useState<WatchProgress[]>([]);
  const [isCurrentBookmarked, setIsCurrentBookmarked] = React.useState(false);

  // Loads providers and the first provider's home page.
  const refreshProviders = React.useCallback(async () => {
    const provs = await api.getProviders();
    setProviders(provs);
    const first = provs[0]?.name || '';
    setSelectedProvider((current) => (provs.some((p) => p.name === current) ? current : first));
    setHomeSections(first ? await api.getHomeSections(first) : []);
  }, []);

  // Initial load
  React.useEffect(() => {
    async function init() {
      try {
        await refreshProviders();
        const [bms, cw] = await Promise.all([api.getBookmarks(), api.getContinueWatching()]);
        setBookmarks(bms);
        setContinueWatching(cw);
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
  const handlePlayEpisode = guard(async (ep: Episode) => {
    if (!activeDetails) throw new Error('Open a title before playing an episode.');
    await playback.start(activeDetails, ep);
  });

  const handlePlayItemDirect = guard(async (item: HomeItem) => {
    const details = await api.getDetails(item.provider, item.url);
    const target = details.episodes[0] ?? movieAsEpisode(details);
    if (!target) {
      throw new Error('This title has nothing to play.');
    }
    await playback.start(details, target);
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
    await playback.start(details, target);
  });

  // Stop player & return
  const handleStopPlayer = guard(async () => {
    try {
      await playback.stop();
    } finally {
      setContinueWatching(await api.getContinueWatching());
    }
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

  if (playback.isOpen) {
    return (
      <>
        <ErrorToast message={errorText} onDismiss={() => setErrorText(null)} />
        <PlayerOverlay
          title={playback.title}
          status={playback.status}
          sources={playback.sources}
          onBack={handleStopPlayer}
          onTogglePlay={guard(playback.togglePlay)}
          onSeek={guard(playback.seek)}
          onSelectTrack={guard(playback.selectTrack)}
          onSelectSource={guard(playback.switchSource)}
          onVolume={guard(playback.setVolume)}
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
                query={searchQuery}
                onQueryChange={setSearchQuery}
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
              <SettingsScreen providers={providers} onError={(error) => setErrorText(describeError(error))} />
            )}
          </>
        )}
      </main>
    </div>
  );
};
export default App;
