import React from 'react';
import { api } from '../services/api';
import { playbackTitle } from '../playback/episodeLookup';
import { CheckProgress, Episode, MediaDetails, PlaybackStatus, SourceInfo } from '../types';
import { onBackendEvent } from '../utils/backendEvents';

/** How often the player state is read while the player is open. */
const STATUS_POLL_MS = 800;

const IDLE_STATUS: PlaybackStatus = {
  state: 'Idle',
  position_ms: 0,
  duration_ms: 0,
  has_video: false,
  volume: 100,
  tracks: [],
  error: null,
};

/** Shown at once when a stream starts or switches, until the player reports its own state. */
const LOADING_STATUS: PlaybackStatus = { ...IDLE_STATUS, state: 'Loading', has_video: true };

/** The check shown from the moment a video is opened, before the backend has counted any streams. */
const OPENING: CheckProgress = {
  checked: 0,
  total: 0,
  working: 0,
  skipping: false,
  discovering: true,
  saved: 'none',
};

/** Everything about the video player screen: what is playing, its state, and the actions on it. */
export function usePlayback() {
  const [isOpen, setIsOpen] = React.useState(false);
  const [title, setTitle] = React.useState('');
  const [status, setStatus] = React.useState<PlaybackStatus>(IDLE_STATUS);
  const [sources, setSources] = React.useState<SourceInfo[]>([]);
  /** Progress of the stream check before playback; null once the video is playing. */
  const [check, setCheck] = React.useState<CheckProgress | null>(null);
  /** Counts openings and closings, so an answer for a video the viewer already left is ignored. */
  const openingRef = React.useRef(0);

  React.useEffect(() => {
    if (!isOpen) return undefined;
    const timer = setInterval(async () => setStatus(await api.getStatus()), STATUS_POLL_MS);
    return () => clearInterval(timer);
  }, [isOpen]);

  const refreshSources = React.useCallback(async () => {
    const opening = openingRef.current;
    const list = await api.getSources().catch(() => []);
    // The answer belongs to the video that was open when it was asked for.
    if (openingRef.current === opening) setSources(list);
  }, []);

  // While the player is open the backend keeps testing the streams; broken ones drop out of the list.
  React.useEffect(() => {
    if (!isOpen) return undefined;
    const stopSources = onBackendEvent('sources-changed', () => void refreshSources());
    const stopProgress = onBackendEvent<CheckProgress>('source-check-progress', (progress) =>
      setCheck((current) => (current ? progress : current)),
    );
    return () => {
      stopSources();
      stopProgress();
    };
  }, [isOpen, refreshSources]);

  const close = () => {
    openingRef.current += 1;
    setIsOpen(false);
    setCheck(null);
    setStatus(IDLE_STATUS);
    setSources([]);
  };

  /** Plays one episode (or the single stream of a movie) of `details`: the player opens at once and shows the check. */
  const start = async (details: MediaDetails, episode: Episode) => {
    const name = playbackTitle(details.name, episode.name);
    const opening = openingRef.current + 1;
    openingRef.current = opening;
    setTitle(name);
    setStatus(IDLE_STATUS);
    setSources([]);
    setCheck(OPENING);
    setIsOpen(true);
    try {
      await api.playMedia({
        provider: details.provider,
        media_key: episode.data,
        title: name,
        detail_url: details.url,
        poster_url: episode.poster_url || details.poster_url || undefined,
      });
    } catch (error) {
      if (openingRef.current !== opening) return; // the viewer already left
      close();
      await api.stop().catch(() => undefined);
      throw error;
    }
    if (openingRef.current !== opening) return;
    setCheck(null);
    setStatus(LOADING_STATUS);
    await refreshSources();
  };

  /** Leaves the player screen at once; the stream is stopped right after, and a failure is reported. */
  const stop = async () => {
    close();
    await api.stop();
  };

  /** Starts now on the best stream known to work; the checks carry on in the background. */
  const skipCheck = async () => {
    setCheck((current) => (current ? { ...current, skipping: true } : current));
    await api.skipSourceCheck();
  };

  const togglePlay = async () => {
    if (status.state === 'Playing') await api.pause();
    else await api.resume();
  };

  const switchSource = async (index: number) => {
    await api.switchSource(index);
    setStatus(LOADING_STATUS);
    await refreshSources();
  };

  return {
    isOpen,
    title,
    status,
    sources,
    check,
    start,
    stop,
    skipCheck,
    togglePlay,
    switchSource,
    seek: (positionMs: number) => api.seekTo(positionMs),
    selectTrack: (kind: 'audio' | 'subtitle', id: number | null) => api.selectTrack(kind, id),
    setVolume: (percent: number) => api.setVolume(percent),
  };
}
