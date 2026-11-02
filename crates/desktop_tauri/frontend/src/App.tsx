import React from 'react';
import { api } from './services/api';
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

export const App: React.FC = () => {
  const [currentTab, setCurrentTab] = React.useState('home');
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

  // Initial load
  React.useEffect(() => {
    async function init() {
      try {
        const provs = await api.getProviders();
        setProviders(provs);
        const first = provs[0]?.name || '';
        setSelectedProvider(first);

        const [sections, bms, cw, repos] = await Promise.all([
          api.getHomeSections(first || undefined),
          api.getBookmarks(),
          api.getContinueWatching(),
          api.getRepositories(),
        ]);
        setHomeSections(sections);
        setBookmarks(bms);
        setContinueWatching(cw);
        setRepositories(repos);
      } finally {
        setHomeLoading(false);
      }
    }
    init();
  }, []);

  // Switch provider
  const handleSelectProvider = async (p: string) => {
    setSelectedProvider(p);
    setHomeLoading(true);
    try {
      const sections = await api.getHomeSections(p);
      setHomeSections(sections);
    } finally {
      setHomeLoading(false);
    }
  };

  // Open Details Screen
  const handleSelectItem = async (item: HomeItem) => {
    setDetailsLoading(true);
    try {
      const details = await api.getDetails(item.provider, item.url);
      setActiveDetails(details);
      const bookmarked = await api.isBookmarked(details.provider, details.url);
      setIsCurrentBookmarked(bookmarked);
    } finally {
      setDetailsLoading(false);
    }
  };

  // Start Playback
  const handlePlayEpisode = async (ep: Episode, mediaName?: string) => {
    const parentTitle = mediaName || activeDetails?.name || 'Media';
    setActiveEpisode(ep);
    setActivePlayerTitle(`${parentTitle} - ${ep.name}`);
    await api.playMedia({
      provider: activeDetails?.provider || selectedProvider,
      media_key: ep.data,
      title: `${parentTitle} - ${ep.name}`,
      detail_url: activeDetails?.url,
      poster_url: ep.poster_url || activeDetails?.poster_url || undefined,
    });
  };

  const handlePlayItemDirect = async (item: HomeItem) => {
    const details = await api.getDetails(item.provider, item.url);
    const firstEp = details.episodes[0];
    if (firstEp) {
      handlePlayEpisode(firstEp, details.name);
    }
  };

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
  const handleStopPlayer = async () => {
    await api.stop();
    setActiveEpisode(null);
    // Refresh continue watching
    const cw = await api.getContinueWatching();
    setContinueWatching(cw);
  };

  // Toggle bookmark
  const handleToggleBookmark = async () => {
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
  };

  const handleRemoveBookmark = async (provider: string, url: string) => {
    await api.removeBookmark(provider, url);
    const bms = await api.getBookmarks();
    setBookmarks(bms);
  };

  // If player is active, render 100% transparent overlay over mpv
  if (activeEpisode) {
    return (
      <PlayerOverlay
        title={activePlayerTitle}
        status={playerStatus}
        onBack={handleStopPlayer}
        onTogglePlay={async () => {
          if (playerStatus.state === 'Playing') {
            await api.pause();
          } else {
            await api.resume();
          }
        }}
        onSeek={async (pos) => {
          await api.seekTo(pos);
        }}
        onSelectTrack={async (kind, id) => {
          await api.selectTrack(kind, id);
        }}
      />
    );
  }

  return (
    <div className="flex flex-col h-screen w-screen bg-midnight text-white overflow-hidden">
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
            onPlayEpisode={(ep) => handlePlayEpisode(ep, activeDetails.name)}
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
                onRemoveBookmark={handleRemoveBookmark}
              />
            )}
            {currentTab === 'settings' && (
              <SettingsScreen
                repositories={repositories}
                providers={providers}
                onAddRepo={async (url) => {
                  await api.addRepository(url);
                  const repos = await api.getRepositories();
                  setRepositories(repos);
                }}
                onRemoveRepo={async (url) => {
                  await api.removeRepository(url);
                  const repos = await api.getRepositories();
                  setRepositories(repos);
                }}
              />
            )}
          </>
        )}
      </main>
    </div>
  );
};
export default App;
