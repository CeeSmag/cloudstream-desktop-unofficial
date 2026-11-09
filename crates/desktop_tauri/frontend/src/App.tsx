import React from 'react';
import { listen } from '@tauri-apps/api/event';
import { api, describeError } from './services/api';
import { ErrorToast } from './components/ErrorToast';
import { CloudflarePanel } from './components/CloudflarePanel';
import { DetailsLoading } from './components/DetailsLoading';
import { CloudflareAsk } from './components/CloudflareAsk';
import { failureAction } from './utils/failureAction';
import { Bookmark, BrowseFilters, CloudflareNeeded, BrowseItem, BrowsePageResult, Episode, HomeItem, HomeSection, MediaDetails, ProviderInfo, WatchProgress } from './types';
import { Navigation } from './components/Navigation';
import { HomeScreen } from './screens/HomeScreen';
import { homeEmptyState } from './utils/homeEmptyState';
import { createLatestRequest } from './utils/latestRequest';
import { BrowseScreen } from './screens/BrowseScreen';
import { SearchScreen } from './screens/SearchScreen';
import { DetailsScreen } from './screens/DetailsScreen';
import { PlayerOverlay } from './screens/PlayerOverlay';
import { LibraryScreen } from './screens/LibraryScreen';
import { SettingsScreen } from './screens/SettingsScreen';
import { usePlayback } from './hooks/usePlayback';
import { useStartup } from './hooks/useStartup';
import { logEvent, setDebugLogging } from './utils/logger';
import { StartupScreen } from './components/StartupScreen';
import { ContextMenu } from './components/ContextMenu';
import { useContextMenu } from './hooks/useContextMenu';
import { resumeTarget } from './playback/resumeTarget';
import { canOpenTitlePage, missingPageMessage, titlePageItem } from './utils/continueWatching';
import { defaultBrowseFilters, pickBestSearchMatch } from './utils/browseFilters';

export const App: React.FC = () => {
  const [currentTab, setCurrentTab] = React.useState('home');
  const [settingsRequest, setSettingsRequest] = React.useState({ page: 'general', nonce: 0 });
  // The tab bar answers a click at once; the page below follows in a render that does not block it.
  const shownTab = React.useDeferredValue(currentTab);
  const [errorText, setErrorText] = React.useState<string | null>(null);
  const [searchQuery, setSearchQuery] = React.useState('');
  const [browseFilters, setBrowseFilters] = React.useState<BrowseFilters>(defaultBrowseFilters);
  const [browseData, setBrowseData] = React.useState<BrowsePageResult | null>(null);
  const [browseFilterKey, setBrowseFilterKey] = React.useState('');
  const handleCacheBrowseData = React.useCallback((key: string, data: BrowsePageResult) => {
    setBrowseFilterKey(key);
    setBrowseData(data);
  }, []);

  // The viewer is asked to verify because something failed meanwhile; the failed action runs again afterwards.
  const [askToVerify, setAskToVerify] = React.useState(false);
  const retryAfterVerifyRef = React.useRef<(() => Promise<void>) | null>(null);

  // Runs an async action and shows its failure instead of dropping it.
  const guard = React.useCallback(
    <A extends unknown[]>(action: (...args: A) => Promise<void>) =>
      async (...args: A): Promise<void> => {
        try {
          await action(...args);
        } catch (error) {
          if (failureAction(cloudflareNeededRef.current) === 'ask-to-verify') {
            // The site is waiting for the viewer's go-ahead: say so, and try again once it is verified.
            logEvent('info', `an action failed while ${cloudflareNeededRef.current?.host} waits for verification; asking the viewer to verify (${describeError(error)})`);
            retryAfterVerifyRef.current = () => action(...args);
            setAskToVerify(true);
            return;
          }
          setErrorText(describeError(error));
        }
      },
    [],
  );
  const [providers, setProviders] = React.useState<ProviderInfo[]>([]);
  const [selectedProvider, setSelectedProvider] = React.useState('');
  // The latest pick, for callbacks that run later (plugins finishing their loading in the background).
  const selectedProviderRef = React.useRef('');
  selectedProviderRef.current = selectedProvider;
  const [homeSections, setHomeSections] = React.useState<HomeSection[]>([]);
  const [homeLoading, setHomeLoading] = React.useState(true);
  // Loading a Home page replaces the one before it; an answer to an older request must not show.
  const homeRequests = React.useRef(createLatestRequest()).current;
  // A site that needs Cloudflare's check and waits for the viewer's go-ahead (instead of a window popping up by itself).
  const [cloudflareNeeded, setCloudflareNeeded] = React.useState<CloudflareNeeded | null>(null);
  const [cloudflareBusy, setCloudflareBusy] = React.useState(false);
  const cloudflareNeededRef = React.useRef<CloudflareNeeded | null>(null);
  // Plugins installed on this PC (null until known); decides whether Home asks the viewer to add some.
  const [installedCount, setInstalledCount] = React.useState<number | null>(null);

  // Navigation states
  const [activeDetails, setActiveDetails] = React.useState<MediaDetails | null>(null);
  const [detailsLoading, setDetailsLoading] = React.useState(false);

  const playback = usePlayback((error) => setErrorText(describeError(error)));
  const startup = useStartup();
  const contextMenu = useContextMenu();

  // Library & settings
  const [bookmarks, setBookmarks] = React.useState<Bookmark[]>([]);
  const [continueWatching, setContinueWatching] = React.useState<WatchProgress[]>([]);
  const [isCurrentBookmarked, setIsCurrentBookmarked] = React.useState(false);

  // Loads providers and the first provider's home page.
  const refreshProviders = React.useCallback(async () => {
    const provs = await api.getProviders();
    setProviders(provs);
    // Keep the provider the viewer picked when it is still there; else the first one.
    const picked = selectedProviderRef.current;
    const target = provs.some((p) => p.name === picked) ? picked : provs[0]?.name || '';
    setSelectedProvider(target);
    const isCurrent = homeRequests.begin();
    try {
      const sections = target ? await api.getHomeSections(target) : [];
      if (isCurrent()) {
        setHomeSections(sections);
        if (sections.length > 0) setCloudflareNeeded(null);
      }
    } catch (error) {
      // The reason is shown, not an empty page that looks like nothing happened; a Cloudflare check
      // that waits for the viewer's go-ahead has its own card on Home instead.
      if (isCurrent()) {
        setHomeSections([]);
        if (!cloudflareNeededRef.current) setErrorText(describeError(error));
      }
    }
    setInstalledCount(await api.getInstalledExtensions().then((list) => list.length).catch(() => null));
  }, []);

  // The saved debug logging choice decides whether this page sends detailed logs.
  React.useEffect(() => {
    api
      .getDebugLogging()
      .then((info) => setDebugLogging(info.enabled))
      .catch(() => undefined);
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

  // The sidecar needs a site verified: the viewer is asked, instead of a window opening by itself.
  React.useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<CloudflareNeeded>('cloudflare-needed', (event) => {
      cloudflareNeededRef.current = event.payload;
      setCloudflareNeeded(event.payload);
    })
      .then((stop) => {
        unlisten = stop;
      })
      .catch(() => undefined);
    return () => unlisten?.();
  }, []);

  // The viewer pressed Verify: the check window may open now, and Home loads again.
  const handleVerifyCloudflare = guard(async () => {
    const needed = cloudflareNeededRef.current;
    if (!needed) return;
    setCloudflareBusy(true);
    try {
      await api.allowCloudflareHost(needed.host);
      cloudflareNeededRef.current = null;
      setCloudflareNeeded(null);
      setAskToVerify(false);
      setHomeLoading(true);
      await refreshProviders();
    } finally {
      setHomeLoading(false);
      setCloudflareBusy(false);
    }
    // Whatever failed while the site waited for this go-ahead is tried again now.
    const retry = retryAfterVerifyRef.current;
    retryAfterVerifyRef.current = null;
    if (retry) {
      logEvent('info', 'verification done; running again the action that failed before it');
      await retry();
    }
  });

  // Switch provider
  const handleSelectProvider = guard(async (p: string) => {
    // Before anything else: from now on every video is searched afresh, never from saved links.
    if (p !== selectedProvider) await api.providerSwitched(selectedProvider, p);
    setSelectedProvider(p);
    setActiveDetails(null);
    setBrowseData(null);
    setBrowseFilterKey('');
    // Start from nothing: the old provider's rows must not stay under the new provider's name.
    setHomeSections([]);
    cloudflareNeededRef.current = null;
    setCloudflareNeeded(null);
    setHomeLoading(true);
    const isCurrent = homeRequests.begin();
    try {
      const sections = await api.getHomeSections(p);
      if (isCurrent()) setHomeSections(sections);
    } finally {
      if (isCurrent()) setHomeLoading(false);
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

  // Open details for an item selected on the Browse screen
  const handleSelectBrowseItem = guard(async (item: BrowseItem) => {
    setDetailsLoading(true);
    try {
      const results = await api.search(item.name, selectedProvider || undefined);
      const match = pickBestSearchMatch(item, results);
      if (match) {
        const details = await api.getDetails(match.provider, match.url);
        setActiveDetails(details);
        const bookmarked = await api.isBookmarked(details.provider, details.url);
        setIsCurrentBookmarked(bookmarked);
      } else {
        setSearchQuery(item.name);
        setCurrentTab('search');
      }
    } finally {
      setDetailsLoading(false);
    }
  });

  // Start Playback
  const handlePlayEpisode = guard(async (ep: Episode) => {
    if (!activeDetails) throw new Error('Open a title before playing an episode.');
    await playback.start(activeDetails, ep);
  });

  // A Continue Watching card opens the title page; playing happens only there.
  const handleOpenContinue = guard(async (progress: WatchProgress) => {
    if (!canOpenTitlePage(progress)) throw new Error(missingPageMessage(progress));
    await handleSelectItem(titlePageItem(progress));
  });

  // The Home banner plays the title from where it was left, without leaving the Home page.
  const handleResumeContinue = guard(async (progress: WatchProgress) => {
    const { details, episode } = await resumeTarget(progress);
    await playback.start(details, episode);
  });

  // Takes a title out of Continue Watching; the backend also forgets its saved sources and subtitles.
  const handleRemoveContinue = guard(async (progress: WatchProgress) => {
    await api.removeContinueWatching(progress.provider, progress.detail_url);
    setContinueWatching(await api.getContinueWatching());
  });

  const handleClearContinue = guard(async () => {
    await api.clearContinueWatching();
    setContinueWatching(await api.getContinueWatching());
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

  if (!startup.ready) {
    return <StartupScreen status={startup} />;
  }

  if (playback.isOpen) {
    return (
      <>
        <ErrorToast message={errorText} onDismiss={() => setErrorText(null)} />
      <CloudflarePanel />
        <ContextMenu menu={contextMenu.menu} onClose={contextMenu.close} />
        <PlayerOverlay
          title={playback.title}
          status={playback.status}
          sources={playback.sources}
          check={playback.check}
          onSkipCheck={guard(playback.skipCheck)}
          onBack={handleStopPlayer}
          onTogglePlay={guard(playback.togglePlay)}
          onSeek={guard(playback.seek)}
          onSelectTrack={guard(playback.selectTrack)}
          onSelectSource={guard(playback.switchSource)}
          onVolume={guard(playback.setVolume)}
          onPrevious={playback.hasPrevious ? guard(playback.playPrevious) : undefined}
          onNext={playback.hasNext ? guard(playback.playNext) : undefined}
          skipSegments={playback.skipSegments}
        />
      </>
    );
  }

  return (
    <div className="flex flex-col h-screen w-screen bg-midnight text-white overflow-hidden">
      <ErrorToast message={errorText} onDismiss={() => setErrorText(null)} />
      <CloudflarePanel />
      {askToVerify && cloudflareNeeded && (
        <CloudflareAsk
          host={cloudflareNeeded.host}
          provider={cloudflareNeeded.provider}
          busy={cloudflareBusy}
          onVerify={handleVerifyCloudflare}
          onClose={() => {
            retryAfterVerifyRef.current = null;
            setAskToVerify(false);
          }}
        />
      )}
      <ContextMenu menu={contextMenu.menu} onClose={contextMenu.close} />
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
        onOpenUpdates={() => {
          setActiveDetails(null);
          setSettingsRequest((r) => ({ page: 'general', nonce: r.nonce + 1 }));
          setCurrentTab('settings');
        }}
      />

      {/* Main View Area */}
      <main className="flex-1 flex overflow-hidden">
        {detailsLoading ? (
          <DetailsLoading />
        ) : activeDetails ? (
          <DetailsScreen
            details={activeDetails}
            onBack={() => setActiveDetails(null)}
            onPlayEpisode={handlePlayEpisode}
            isBookmarked={isCurrentBookmarked}
            onToggleBookmark={handleToggleBookmark}
          />
        ) : (
          <div key={shownTab} className="screen-in flex flex-1 min-w-0 overflow-hidden">
            {shownTab === 'home' && (
              <HomeScreen
                selectedProvider={selectedProvider}
                cloudflareNeeded={cloudflareNeeded}
                cloudflareBusy={cloudflareBusy}
                onVerifyCloudflare={handleVerifyCloudflare}
                emptyState={homeEmptyState({ loading: homeLoading, providerCount: providers.length, installedCount })}
                onOpenExtensions={() => {
                  setSettingsRequest((r) => ({ page: 'extensions', nonce: r.nonce + 1 }));
                  setCurrentTab('settings');
                }}
                sections={homeSections}
                onSelectItem={handleSelectItem}
                loading={homeLoading}
                continueWatching={continueWatching}
                onOpenContinue={handleOpenContinue}
                onResumeContinue={handleResumeContinue}
                onRemoveContinue={handleRemoveContinue}
              />
            )}
            {shownTab === 'browse' && (
              <BrowseScreen
                filters={browseFilters}
                selectedProvider={selectedProvider}
                onUpdateFilters={setBrowseFilters}
                onSelectItem={handleSelectBrowseItem}
                onError={(err) => setErrorText(err)}
                cachedData={browseData}
                cachedFilterKey={browseFilterKey}
                onCacheData={handleCacheBrowseData}
              />
            )}
            {shownTab === 'search' && (
              <SearchScreen
                onSelectItem={handleSelectItem}
                selectedProvider={selectedProvider}
                query={searchQuery}
                onQueryChange={setSearchQuery}
                onSearch={(q) => api.search(q, selectedProvider || undefined)}
              />
            )}
            {shownTab === 'library' && (
              <LibraryScreen
                bookmarks={bookmarks}
                continueWatching={continueWatching}
                onSelectItem={handleSelectItem}
                onOpenContinue={handleOpenContinue}
                onRemoveContinue={handleRemoveContinue}
                onClearContinue={handleClearContinue}
                onRemoveBookmark={handleRemoveBookmark}
              />
            )}
            {shownTab === 'settings' && (
              <SettingsScreen providers={providers} request={settingsRequest} onError={(error) => setErrorText(describeError(error))} />
            )}
          </div>
        )}
      </main>
    </div>
  );
};
export default App;
