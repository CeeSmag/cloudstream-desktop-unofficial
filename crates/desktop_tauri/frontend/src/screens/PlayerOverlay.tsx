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
  Loader2,
} from 'lucide-react';
import { PlaybackStatus } from '../types';
import { VideoCanvas } from '../components/VideoCanvas';

interface PlayerOverlayProps {
  title: string;
  status: PlaybackStatus;
  onBack: () => void;
  onTogglePlay: () => void;
  onSeek: (positionMs: number) => void;
  onSelectTrack: (kind: 'audio' | 'subtitle', id: number | null) => void;
}

export const PlayerOverlay: React.FC<PlayerOverlayProps> = ({
  title,
  status,
  onBack,
  onTogglePlay,
  onSeek,
  onSelectTrack,
}) => {
  const [controlsVisible, setControlsVisible] = React.useState(true);
  const [trackMenu, setTrackMenu] = React.useState<'none' | 'subtitles' | 'audio'>('none');
  const [isFullscreen, setIsFullscreen] = React.useState(false);
  const [volume, setVolume] = React.useState(100);
  const [isMuted, setIsMuted] = React.useState(false);
  const hideTimerRef = React.useRef<number | null>(null);

  const isPlaying = status.state === 'Playing';
  const isLoading = status.state === 'Loading';

  const resetHideTimer = React.useCallback(() => {
    setControlsVisible(true);
    if (hideTimerRef.current) clearTimeout(hideTimerRef.current);
    if (isPlaying) {
      hideTimerRef.current = window.setTimeout(() => {
        setControlsVisible(false);
        setTrackMenu('none');
      }, 3500);
    }
  }, [isPlaying]);

  React.useEffect(() => {
    resetHideTimer();
    const handleMouseMove = () => resetHideTimer();
    window.addEventListener('mousemove', handleMouseMove);
    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      if (hideTimerRef.current) clearTimeout(hideTimerRef.current);
    };
  }, [resetHideTimer]);

  const formatTime = (ms: number) => {
    const totalSec = Math.floor(Math.max(0, ms) / 1000);
    const m = Math.floor(totalSec / 60);
    const s = totalSec % 60;
    const h = Math.floor(m / 60);
    if (h > 0) {
      const remM = m % 60;
      return `${h}:${remM < 10 ? '0' : ''}${remM}:${s < 10 ? '0' : ''}${s}`;
    }
    return `${m}:${s < 10 ? '0' : ''}${s}`;
  };

  const progressPercent = status.duration_ms > 0
    ? Math.min(100, Math.max(0, (status.position_ms / status.duration_ms) * 100))
    : 0;

  const handleSeekClick = (e: React.MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    onSeek(Math.floor(ratio * status.duration_ms));
  };

  const audioTracks = status.tracks.filter((t) => t.kind === 'audio');
  const subtitleTracks = status.tracks.filter((t) => t.kind === 'subtitle');

  return (
    <div
      className={`fixed inset-0 z-50 bg-black flex flex-col justify-between transition-opacity duration-300 select-none ${
        controlsVisible ? 'cursor-default' : 'cursor-none'
      }`}
      onClick={(e) => {
        if (e.target === e.currentTarget) {
          onTogglePlay();
        }
      }}
    >
      <VideoCanvas visible={status.has_video} />

      {/* Top Floating Glass Bar */}
      <div
        className={`px-8 py-5 flex items-center justify-between bg-gradient-to-b from-midnight/90 via-midnight/40 to-transparent transition-opacity duration-300 ${
          controlsVisible ? 'opacity-100' : 'opacity-0 pointer-events-none'
        }`}
      >
        <div className="flex items-center space-x-4">
          <button
            onClick={onBack}
            className="p-2.5 rounded-xl bg-white/10 hover:bg-white/20 text-white border border-white/10 transition-colors backdrop-blur-md shadow-lg"
            title="Exit Player"
          >
            <ArrowLeft className="w-5 h-5" />
          </button>
          <div>
            <h2 className="text-base font-bold text-white tracking-tight drop-shadow-md">
              {title}
            </h2>
            <p className="text-xs text-zinc-400 drop-shadow-sm font-medium">
              Hardware-accelerated libmpv
            </p>
          </div>
        </div>

        {/* Top Right Quick Actions */}
        <div className="flex items-center space-x-2">
          {/* Subtitles Button */}
          <div className="relative">
            <button
              onClick={() => setTrackMenu(trackMenu === 'subtitles' ? 'none' : 'subtitles')}
              className={`p-2.5 rounded-xl border transition-colors backdrop-blur-md ${
                trackMenu === 'subtitles'
                  ? 'bg-accent border-accent text-white'
                  : 'bg-white/10 hover:bg-white/20 border-white/10 text-white'
              }`}
              title="Subtitles"
            >
              <Subtitles className="w-5 h-5" />
            </button>

            {trackMenu === 'subtitles' && (
              <div className="absolute right-0 mt-2 w-56 rounded-xl glass-dropdown p-2 shadow-2xl z-50">
                <div className="px-3 py-1 text-[11px] font-semibold text-zinc-400 uppercase tracking-wider">
                  Subtitles
                </div>
                <button
                  onClick={() => {
                    onSelectTrack('subtitle', null);
                    setTrackMenu('none');
                  }}
                  className="w-full text-left px-3 py-2 text-xs rounded-lg text-zinc-300 hover:bg-white/5 transition-colors"
                >
                  Off
                </button>
                {subtitleTracks.map((t) => (
                  <button
                    key={t.id}
                    onClick={() => {
                      onSelectTrack('subtitle', t.id);
                      setTrackMenu('none');
                    }}
                    className={`w-full text-left px-3 py-2 text-xs rounded-lg flex items-center justify-between transition-colors ${
                      t.selected ? 'bg-accent/20 text-accent font-semibold' : 'text-zinc-300 hover:bg-white/5'
                    }`}
                  >
                    <span className="truncate">{t.title || t.language || `Track ${t.id}`}</span>
                    {t.selected && <span className="w-1.5 h-1.5 rounded-full bg-accent" />}
                  </button>
                ))}
              </div>
            )}
          </div>

          {/* Audio Tracks Button */}
          <div className="relative">
            <button
              onClick={() => setTrackMenu(trackMenu === 'audio' ? 'none' : 'audio')}
              className={`p-2.5 rounded-xl border transition-colors backdrop-blur-md ${
                trackMenu === 'audio'
                  ? 'bg-accent border-accent text-white'
                  : 'bg-white/10 hover:bg-white/20 border-white/10 text-white'
              }`}
              title="Audio Tracks"
            >
              <AudioLines className="w-5 h-5" />
            </button>

            {trackMenu === 'audio' && (
              <div className="absolute right-0 mt-2 w-56 rounded-xl glass-dropdown p-2 shadow-2xl z-50">
                <div className="px-3 py-1 text-[11px] font-semibold text-zinc-400 uppercase tracking-wider">
                  Audio Track
                </div>
                {audioTracks.map((t) => (
                  <button
                    key={t.id}
                    onClick={() => {
                      onSelectTrack('audio', t.id);
                      setTrackMenu('none');
                    }}
                    className={`w-full text-left px-3 py-2 text-xs rounded-lg flex items-center justify-between transition-colors ${
                      t.selected ? 'bg-accent/20 text-accent font-semibold' : 'text-zinc-300 hover:bg-white/5'
                    }`}
                  >
                    <span className="truncate">{t.title || t.language || `Track ${t.id}`}</span>
                    {t.selected && <span className="w-1.5 h-1.5 rounded-full bg-accent" />}
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Center Spinner or Play Feedback */}
      {isLoading && (
        <div className="self-center flex flex-col items-center justify-center p-6 rounded-2xl bg-midnight/70 backdrop-blur-md border border-white/10 shadow-2xl">
          <Loader2 className="w-10 h-10 text-accent animate-spin mb-3" />
          <span className="text-sm font-medium text-zinc-200">Loading stream...</span>
        </div>
      )}

      {status.state === 'Failed' && (
        <div
          role="alert"
          className="self-center max-w-md flex flex-col items-center p-6 text-center rounded-2xl bg-red-950/80 backdrop-blur-md border border-red-500/30 shadow-2xl"
        >
          <span className="text-sm font-medium text-red-100">
            {status.error ?? 'This stream could not be played.'}
          </span>
          <button
            onClick={onBack}
            className="mt-4 px-4 py-2 rounded-lg bg-white/10 hover:bg-white/20 text-white text-sm"
          >
            Go back
          </button>
        </div>
      )}

      {/* Bottom Floating Glass Bar */}
      <div
        className={`px-8 pb-6 pt-12 bg-gradient-to-t from-midnight/95 via-midnight/60 to-transparent transition-opacity duration-300 ${
          controlsVisible ? 'opacity-100' : 'opacity-0 pointer-events-none'
        }`}
      >
        <div className="max-w-5xl mx-auto space-y-3.5">
          {/* Seek Bar */}
          <div
            onClick={handleSeekClick}
            className="group relative h-2 hover:h-3 rounded-full bg-white/20 hover:bg-white/25 cursor-pointer transition-all duration-150"
          >
            {/* Progress Fill */}
            <div
              className="absolute left-0 top-0 bottom-0 rounded-full bg-gradient-to-r from-accent to-purple-400 shadow-sm"
              style={{ width: `${progressPercent}%` }}
            />
            {/* Thumb */}
            <div
              className="absolute top-1/2 -translate-y-1/2 -ml-2 w-4 h-4 rounded-full bg-white shadow-lg glow-accent opacity-0 group-hover:opacity-100 transition-opacity"
              style={{ left: `${progressPercent}%` }}
            />
          </div>

          {/* Controls Bar Row */}
          <div className="flex items-center justify-between">
            {/* Left Controls: Play/Pause, Skips, Time */}
            <div className="flex items-center space-x-3">
              <button
                onClick={onTogglePlay}
                className="w-11 h-11 rounded-xl bg-accent hover:bg-accent-hover text-white flex items-center justify-center shadow-lg glow-accent transition-transform duration-150 active:scale-95"
              >
                {isPlaying ? (
                  <Pause className="w-5 h-5 fill-current" />
                ) : (
                  <Play className="w-5 h-5 fill-current ml-0.5" />
                )}
              </button>

              <button
                onClick={() => onSeek(status.position_ms - 10000)}
                className="p-2.5 rounded-xl bg-white/10 hover:bg-white/20 text-white border border-white/10 transition-colors"
                title="Rewind 10s"
              >
                <RotateCcw className="w-4 h-4" />
              </button>

              <button
                onClick={() => onSeek(status.position_ms + 10000)}
                className="p-2.5 rounded-xl bg-white/10 hover:bg-white/20 text-white border border-white/10 transition-colors"
                title="Forward 10s"
              >
                <RotateCw className="w-4 h-4" />
              </button>

              <span className="text-xs font-semibold text-zinc-300 ml-2 tracking-wide font-mono">
                {formatTime(status.position_ms)} / {formatTime(status.duration_ms)}
              </span>
            </div>

            {/* Right Controls: Volume, Fullscreen */}
            <div className="flex items-center space-x-3">
              {/* Volume Group */}
              <div className="flex items-center space-x-2 px-3 py-1.5 rounded-xl bg-white/10 border border-white/10">
                <button
                  onClick={() => setIsMuted(!isMuted)}
                  className="text-zinc-300 hover:text-white"
                >
                  {isMuted || volume === 0 ? (
                    <VolumeX className="w-4 h-4 text-rose-400" />
                  ) : (
                    <Volume2 className="w-4 h-4" />
                  )}
                </button>
                <input
                  type="range"
                  min="0"
                  max="100"
                  value={isMuted ? 0 : volume}
                  onChange={(e) => {
                    const val = Number(e.target.value);
                    setVolume(val);
                    if (val > 0) setIsMuted(false);
                  }}
                  className="w-18 h-1 accent-accent rounded-lg cursor-pointer"
                />
              </div>

              {/* Fullscreen Toggle */}
              <button
                onClick={() => {
                  if (!document.fullscreenElement) {
                    document.documentElement.requestFullscreen().catch(() => {});
                    setIsFullscreen(true);
                  } else {
                    document.exitFullscreen().catch(() => {});
                    setIsFullscreen(false);
                  }
                }}
                className="p-2.5 rounded-xl bg-white/10 hover:bg-white/20 text-white border border-white/10 transition-colors"
                title="Toggle Fullscreen"
              >
                {isFullscreen ? <Minimize2 className="w-4 h-4" /> : <Maximize2 className="w-4 h-4" />}
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
