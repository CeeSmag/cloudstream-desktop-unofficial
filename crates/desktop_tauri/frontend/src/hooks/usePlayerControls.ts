import React from 'react';
import { PlaybackStatus } from '../types';
import { clampVolume, loadVolume, saveVolume } from '../utils/volume';
import { seekText, volumeText } from '../utils/feedbackText';
import { wheelVolumeDelta } from '../utils/wheelVolume';
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
  const { popup, visible: popupVisible, show } = useFeedbackPopup();
  const { fullscreen, toggle } = useWindowFullscreen();
  const [volume, setVolume] = React.useState(loadVolume);
  const [muted, setMuted] = React.useState(false);
  /** The volume now heard, updated at once so a fast burst of wheel turns adds up instead of repeating one step. */
  const level = React.useRef(volume);
  const wheelCarry = React.useRef(0);

  // Pushes the chosen level to the player, also when the screen opens so it matches the saved one.
  React.useEffect(() => {
    onVolume(muted ? 0 : volume);
  }, [volume, muted]); // eslint-disable-line react-hooks/exhaustive-deps

  const changeVolume = (value: number) => {
    const next = clampVolume(value);
    level.current = next;
    setVolume(next);
    saveVolume(next);
    if (next > 0) setMuted(false);
    show({ icon: next > 0 ? 'volume' : 'mute', text: volumeText(next), level: next });
  };

  const toggleMute = () => {
    const next = !muted;
    level.current = next ? 0 : volume;
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
    volumeBy: (delta) => changeVolume(level.current + delta),
    toggleMute,
    toggleFullscreen: () => void toggleFullscreen(),
    onActivity: reveal,
  });

  // The mouse wheel changes the volume; a scrolling menu keeps its own wheel.
  const onWheel = (event: React.WheelEvent) => {
    if ((event.target as HTMLElement).closest?.('[data-wheel-scroll]')) return;
    const step = wheelVolumeDelta(event.deltaY, wheelCarry.current);
    wheelCarry.current = step.carry;
    if (step.delta === 0) return;
    reveal();
    changeVolume(level.current + step.delta);
  };

  // One click pauses or plays; a double click toggles fullscreen without pausing.
  const clicks = useClickOrDoubleClick(togglePlay, () => void toggleFullscreen());

  return {
    popup,
    popupVisible,
    show,
    volume,
    muted,
    fullscreen,
    clicks,
    onWheel,
    changeVolume,
    toggleMute,
    togglePlay,
    seekBy,
    jumpTo,
    toggleFullscreen,
  };
}
