import React from 'react';
import { api } from '../services/api';
import { playbackTitle } from '../playback/episodeLookup';
import { Episode, MediaDetails, PlaybackStatus, SourceInfo } from '../types';

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

/** Everything about the video player screen: what is playing, its state, and the actions on it. */
export function usePlayback() {
  const [isOpen, setIsOpen] = React.useState(false);
  const [title, setTitle] = React.useState('');
  const [status, setStatus] = React.useState<PlaybackStatus>(IDLE_STATUS);
  const [sources, setSources] = React.useState<SourceInfo[]>([]);

  React.useEffect(() => {
    if (!isOpen) return undefined;
    const timer = setInterval(async () => setStatus(await api.getStatus()), STATUS_POLL_MS);
    return () => clearInterval(timer);
  }, [isOpen]);

  const refreshSources = async () => setSources(await api.getSources().catch(() => []));

  /** Plays one episode (or the single stream of a movie) of `details`. */
  const start = async (details: MediaDetails, episode: Episode) => {
    const name = playbackTitle(details.name, episode.name);
    await api.playMedia({
      provider: details.provider,
      media_key: episode.data,
      title: name,
      detail_url: details.url,
      poster_url: episode.poster_url || details.poster_url || undefined,
    });
    setTitle(name);
    setStatus(LOADING_STATUS);
    await refreshSources();
    setIsOpen(true);
  };

  /** Leaves the player screen at once; the stream is stopped right after, and a failure is reported. */
  const stop = async () => {
    setIsOpen(false);
    setStatus(IDLE_STATUS);
    setSources([]);
    await api.stop();
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
    start,
    stop,
    togglePlay,
    switchSource,
    seek: (positionMs: number) => api.seekTo(positionMs),
    selectTrack: (kind: 'audio' | 'subtitle', id: number | null) => api.selectTrack(kind, id),
    setVolume: (percent: number) => api.setVolume(percent),
  };
}
