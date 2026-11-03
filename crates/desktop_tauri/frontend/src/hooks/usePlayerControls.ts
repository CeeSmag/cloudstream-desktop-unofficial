import React from 'react';
import { PlaybackStatus } from '../types';
import { clampVolume, loadVolume, saveVolume } from '../utils/volume';
import { seekText, volumeText } from '../utils/feedbackText';
import { formatTime } from '../utils/playerTime';
import { useClickOrDoubleClick } from './useClickOrDoubleClick';
import { useFeedbackPopup } from './useFeedbackPopup';
import { usePlayerShortcuts } from './usePlayerShortcuts';
import { useWindowFullscreen } from './useWindowFullscreen';

interface PlayerControlsInput {
  status: PlaybackStatus;
  onTogglePlay: () => void;
  onSeek: (positionMs: number) => void;
  onVolume: (percent: number) => void;
  /** Shows the controls, used when a key is pressed. */
  reveal: () => void;
}

/**
 * Everything the viewer can do to the video by mouse or keyboard (play, jump, volume, mute,
 * fullscreen), each with a pop-up in the middle of the player that confirms it.
 */
export function usePlayerControls({ status, onTogglePlay, onSeek, onVolume, reveal }: PlayerControlsInput) {
  const { popup, show } = useFeedbackPopup();
  const { fullscreen, toggle } = useWindowFullscreen();
  const [volume, setVolume] = React.useState(loadVolume);
  const [muted, setMuted] = React.useState(false);

  // Pushes the chosen level to the player, also when the screen opens so it matches the saved one.
  React.useEffect(() => {
    onVolume(muted ? 0 : volume);
  }, [volume, muted]); // eslint-disable-line react-hooks/exhaustive-deps

  const changeVolume = (value: number) => {
    const next = clampVolume(value);
    setVolume(next);
    saveVolume(next);
    if (next > 0) setMuted(false);
    show({ icon: next > 0 ? 'volume' : 'mute', text: volumeText(next), level: next });
  };

  const toggleMute = () => {
    const next = !muted;
    setMuted(next);
    if (next) show({ icon: 'mute', text: 'Muted' });
    else show({ icon: 'volume', text: volumeText(volume), level: volume });
  };

  const togglePlay = () => {
    const playing = status.state === 'Playing';
    show(playing ? { icon: 'pause', text: 'Paused' } : { icon: 'play', text: 'Playing' });
    onTogglePlay();
  };

  const seekBy = (deltaMs: number) => {
    show({ icon: deltaMs < 0 ? 'back' : 'forward', text: seekText(deltaMs) });
    onSeek(Math.max(0, status.position_ms + deltaMs));
  };

  const jumpTo = (positionMs: number) => {
    show({ icon: 'jump', text: `Jump to ${formatTime(positionMs)}` });
    onSeek(positionMs);
  };

  const toggleFullscreen = () => {
    show(fullscreen ? { icon: 'windowed', text: 'Windowed' } : { icon: 'fullscreen', text: 'Fullscreen' });
    return toggle();
  };

  usePlayerShortcuts({
    seekBy,
    volumeBy: (delta) => changeVolume((muted ? 0 : volume) + delta),
    toggleMute,
    toggleFullscreen: () => void toggleFullscreen(),
    onActivity: reveal,
  });

  // One click pauses or plays; a double click toggles fullscreen without pausing.
  const clicks = useClickOrDoubleClick(togglePlay, () => void toggleFullscreen());

  return {
    popup,
    show,
    volume,
    muted,
    fullscreen,
    clicks,
    changeVolume,
    toggleMute,
    togglePlay,
    seekBy,
    jumpTo,
    toggleFullscreen,
  };
}
