import React from 'react';
import {
  ArrowLeft,
  Play,
  Pause,
  RotateCcw,
  RotateCw,
  Volume2,
  VolumeX,
  Maximize2,
  Minimize2,
  Subtitles,
  AudioLines,
  Layers,
  Loader2,
} from 'lucide-react';
import { CheckProgress, MediaTrack, PlaybackStatus, SourceInfo } from '../types';
import { SourceCheckPanel } from '../components/SourceCheckPanel';
import { VideoCanvas } from '../components/VideoCanvas';
import { MenuOption, OptionMenu } from '../components/OptionMenu';
import { SeekBar } from '../components/SeekBar';
import { formatTime } from '../utils/playerTime';
import { clampVolume, loadVolume, saveVolume } from '../utils/volume';
import { numberDuplicateLabels } from '../utils/trackLabels';

interface PlayerOverlayProps {
  title: string;
  status: PlaybackStatus;
  sources: SourceInfo[];
  /** Progress of the stream check before playback; null once the video plays. */
  check: CheckProgress | null;
  onSkipCheck: () => void;
  onBack: () => void;
  onTogglePlay: () => void;
  onSeek: (positionMs: number) => void;
  onSelectTrack: (kind: 'audio' | 'subtitle', id: number | null) => void;
  onSelectSource: (index: number) => void;
  onVolume: (percent: number) => void;
}

/** How long the controls stay visible after the mouse stops moving. */
const CONTROLS_HIDE_DELAY_MS = 3500;
const SKIP_MS = 10000;
const ROUND_BUTTON = 'p-2 sm:p-2.5 rounded-xl bg-white/10 hover:bg-white/20 text-white border border-white/10 transition-colors';

/** The menu rows for the audio or subtitle tracks of the stream. */
function trackOptions(tracks: MediaTrack[], kind: MediaTrack['kind']): MenuOption[] {
  const ofKind = tracks.filter((track) => track.kind === kind);
  const labels = numberDuplicateLabels(ofKind.map((track) => track.title));
  return ofKind.map((track, index) => ({ id: track.id, label: labels[index] ?? track.title, selected: track.selected }));
}

export const PlayerOverlay: React.FC<PlayerOverlayProps> = ({
  title,
  status,
  sources,
  check,
  onSkipCheck,
  onBack,
  onTogglePlay,
  onSeek,
  onSelectTrack,
  onSelectSource,
  onVolume,
}) => {
  const [controlsVisible, setControlsVisible] = React.useState(true);
  const [trackMenu, setTrackMenu] = React.useState<'none' | 'sources' | 'subtitles' | 'audio'>('none');
  const [isFullscreen, setIsFullscreen] = React.useState(false);
  const [volume, setVolume] = React.useState(loadVolume);
  const [isMuted, setIsMuted] = React.useState(false);
  const hideTimerRef = React.useRef<number | null>(null);

  const isPlaying = status.state === 'Playing';

  const resetHideTimer = React.useCallback(() => {
    setControlsVisible(true);
    if (hideTimerRef.current) clearTimeout(hideTimerRef.current);
    if (isPlaying) {
      hideTimerRef.current = window.setTimeout(() => {
        setControlsVisible(false);
        setTrackMenu('none');
      }, CONTROLS_HIDE_DELAY_MS);
    }
  }, [isPlaying]);

  React.useEffect(() => {
    resetHideTimer();
    window.addEventListener('mousemove', resetHideTimer);
    return () => {
      window.removeEventListener('mousemove', resetHideTimer);
      if (hideTimerRef.current) clearTimeout(hideTimerRef.current);
    };
  }, [resetHideTimer]);

  // Pushes the chosen level to the player, also when the screen opens so it matches the saved one.
  React.useEffect(() => {
    onVolume(isMuted ? 0 : volume);
  }, [volume, isMuted]); // eslint-disable-line react-hooks/exhaustive-deps

  const changeVolume = (value: number) => {
    const next = clampVolume(value);
    setVolume(next);
    saveVolume(next);
    if (next > 0) setIsMuted(false);
  };

  const toggleFullscreen = () => {
    if (!document.fullscreenElement) {
      document.documentElement.requestFullscreen().catch(() => {});
      setIsFullscreen(true);
    } else {
      document.exitFullscreen().catch(() => {});
      setIsFullscreen(false);
    }
  };

  const selectTrack = (kind: 'audio' | 'subtitle') => (id: number | null) => {
    onSelectTrack(kind, id);
    setTrackMenu('none');
  };

  const fade = controlsVisible ? 'opacity-100' : 'opacity-0 pointer-events-none';

  return (
    <div
      className={`fixed inset-0 z-50 bg-black flex flex-col justify-between transition-opacity duration-300 select-none ${
        controlsVisible ? 'cursor-default' : 'cursor-none'
      }`}
      onClick={(e) => {
        if (e.target === e.currentTarget) onTogglePlay();
      }}
    >
      <VideoCanvas visible={status.has_video} />

      {/* Top bar: back, title, track menus */}
      <div
        className={`page-gutter py-3 sm:py-5 flex items-center justify-between gap-3 bg-gradient-to-b from-midnight/90 via-midnight/40 to-transparent transition-opacity duration-300 ${fade}`}
      >
        <div className="flex items-center gap-3 sm:gap-4 min-w-0">
          <button onClick={onBack} className={`${ROUND_BUTTON} flex-none backdrop-blur-md shadow-lg`} title="Exit Player">
            <ArrowLeft className="w-5 h-5" />
          </button>
          <h2 className="text-sm sm:text-base font-bold text-white tracking-tight drop-shadow-md truncate">{title}</h2>
        </div>

        <div className="flex items-center gap-2 flex-none">
          <OptionMenu
            label="Sources"
            icon={Layers}
            options={sources.map((source) => ({
              id: source.index,
              label: source.checking ? `${source.label} · checking…` : source.label,
              selected: source.selected,
            }))}
            allowOff={false}
            open={trackMenu === 'sources'}
            onToggle={() => setTrackMenu(trackMenu === 'sources' ? 'none' : 'sources')}
            onSelect={(index) => {
              setTrackMenu('none');
              if (index !== null) onSelectSource(index);
            }}
          />
          <OptionMenu
            label="Subtitles"
            icon={Subtitles}
            options={trackOptions(status.tracks, 'subtitle')}
            allowOff
            open={trackMenu === 'subtitles'}
            onToggle={() => setTrackMenu(trackMenu === 'subtitles' ? 'none' : 'subtitles')}
            onSelect={selectTrack('subtitle')}
          />
          <OptionMenu
            label="Audio Track"
            icon={AudioLines}
            options={trackOptions(status.tracks, 'audio')}
            allowOff={false}
            open={trackMenu === 'audio'}
            onToggle={() => setTrackMenu(trackMenu === 'audio' ? 'none' : 'audio')}
            onSelect={selectTrack('audio')}
          />
        </div>
      </div>

      {check && <SourceCheckPanel progress={check} onSkip={onSkipCheck} />}

      {!check && status.state === 'Loading' && (
        <div className="self-center mx-4 flex flex-col items-center justify-center p-6 rounded-2xl bg-midnight/70 backdrop-blur-md border border-white/10 shadow-2xl">
          <Loader2 className="w-10 h-10 text-accent animate-spin mb-3" />
          <span className="text-sm font-medium text-zinc-200">Loading stream...</span>
        </div>
      )}

      {!status.has_video && (status.state === 'Playing' || status.state === 'Paused') && (
        <div className="self-center mx-4 flex flex-col items-center gap-2 text-zinc-400">
          <AudioLines className="w-12 h-12 text-accent" />
          <span className="text-sm font-medium">Audio only</span>
        </div>
      )}

      {status.state === 'Failed' && (
        <div
          role="alert"
          className="self-center mx-4 max-w-md flex flex-col items-center p-6 text-center rounded-2xl bg-red-950/80 backdrop-blur-md border border-red-500/30 shadow-2xl"
        >
          <span className="text-sm font-medium text-red-100">{status.error ?? 'This stream could not be played.'}</span>
          <button onClick={onBack} className="mt-4 px-4 py-2 rounded-lg bg-white/10 hover:bg-white/20 text-white text-sm">
            Go back
          </button>
        </div>
      )}

      {/* Bottom bar: seek, play controls, volume, fullscreen */}
      <div
        className={`page-gutter pb-4 sm:pb-6 pt-10 [@media(max-height:480px)]:pt-4 bg-gradient-to-t from-midnight/95 via-midnight/60 to-transparent transition-opacity duration-300 ${check ? 'invisible' : fade}`}
      >
        <div className="max-w-5xl 2xl:max-w-7xl mx-auto space-y-1 sm:space-y-2">
          <SeekBar positionMs={status.position_ms} durationMs={status.duration_ms} onSeek={onSeek} />

          <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-2">
            <div className="flex items-center gap-2 sm:gap-3">
              <button
                onClick={onTogglePlay}
                className="w-10 h-10 sm:w-11 sm:h-11 rounded-xl bg-accent hover:bg-accent-hover text-white flex items-center justify-center shadow-lg glow-accent transition-transform duration-150 active:scale-95"
                title={isPlaying ? 'Pause' : 'Play'}
              >
                {isPlaying ? <Pause className="w-5 h-5 fill-current" /> : <Play className="w-5 h-5 fill-current ml-0.5" />}
              </button>
              <button onClick={() => onSeek(status.position_ms - SKIP_MS)} className={ROUND_BUTTON} title="Rewind 10s">
                <RotateCcw className="w-4 h-4" />
              </button>
              <button onClick={() => onSeek(status.position_ms + SKIP_MS)} className={ROUND_BUTTON} title="Forward 10s">
                <RotateCw className="w-4 h-4" />
              </button>
              <span className="text-[11px] sm:text-xs font-semibold text-zinc-300 ml-1 sm:ml-2 tracking-wide font-mono whitespace-nowrap">
                {formatTime(status.position_ms)} / {formatTime(status.duration_ms)}
              </span>
            </div>

            <div className="flex items-center gap-2 sm:gap-3">
              <div className="flex items-center gap-2 px-3 py-1.5 rounded-xl bg-white/10 border border-white/10">
                <button onClick={() => setIsMuted(!isMuted)} className="text-zinc-300 hover:text-white" title="Mute">
                  {isMuted || volume === 0 ? <VolumeX className="w-4 h-4 text-rose-400" /> : <Volume2 className="w-4 h-4" />}
                </button>
                <input
                  type="range"
                  min="0"
                  max="100"
                  value={isMuted ? 0 : volume}
                  onChange={(e) => changeVolume(Number(e.target.value))}
                  className="hidden md:block w-24 h-1 accent-accent rounded-lg cursor-pointer"
                  aria-label="Volume"
                />
              </div>
              <button onClick={toggleFullscreen} className={ROUND_BUTTON} title="Toggle Fullscreen">
                {isFullscreen ? <Minimize2 className="w-4 h-4" /> : <Maximize2 className="w-4 h-4" />}
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
