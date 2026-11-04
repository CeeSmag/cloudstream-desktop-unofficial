import React from 'react';
import { SkipSegment } from '../types';
import { formatTime, progressPercent, seekPositionFromClick } from '../utils/playerTime';
import { HoverInfo, hoverInfo, segmentMarks } from '../utils/seekBarHover';

interface SeekBarProps {
  positionMs: number;
  durationMs: number;
  onSeek: (positionMs: number) => void;
  /** Intro, recap and credits of the video, drawn as coloured stretches on the bar when known. */
  segments?: SkipSegment[];
}

/** The colour of each kind of stretch drawn on the bar. */
const MARK_COLORS: Record<SkipSegment['kind'], string> = {
  intro: 'bg-amber-400/70',
  recap: 'bg-sky-400/70',
  credits: 'bg-emerald-400/70',
  preview: 'bg-fuchsia-400/70',
};

/**
 * The progress strip of the player; clicking it jumps to that point. Pointing at it shows the time
 * of that point (and which stretch it is, when the intro, recap or credits are known); those
 * stretches are drawn on the bar.
 */
export const SeekBar: React.FC<SeekBarProps> = ({ positionMs, durationMs, onSeek, segments = [] }) => {
  const percent = progressPercent(positionMs, durationMs);
  const [hover, setHover] = React.useState<HoverInfo | null>(null);
  const marks = React.useMemo(() => segmentMarks(segments, durationMs), [segments, durationMs]);

  const handleClick = (event: React.MouseEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    onSeek(seekPositionFromClick(event.clientX, rect.left, rect.width, durationMs));
  };

  const handleMove = (event: React.MouseEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    setHover(hoverInfo({ clientX: event.clientX, left: rect.left, width: rect.width, durationMs, segments }));
  };

  return (
    // The tall transparent padding makes the thin bar easy to hit on touch screens and small windows.
    <div
      onClick={handleClick}
      onMouseMove={handleMove}
      onMouseLeave={() => setHover(null)}
      className="group relative py-2 cursor-pointer"
    >
      {hover && (
        <div
          role="tooltip"
          className="pointer-events-none absolute bottom-full mb-1 -translate-x-1/2 whitespace-nowrap rounded-lg bg-black/85 px-2.5 py-1 text-xs font-semibold text-white shadow-lg border border-white/10"
          style={{ left: `${hover.percent}%` }}
        >
          {formatTime(hover.positionMs)}
          {hover.label && <span className="ml-1.5 font-medium text-zinc-300">· {hover.label}</span>}
        </div>
      )}
      <div className="relative h-1.5 sm:h-2 group-hover:h-2.5 rounded-full bg-white/20 group-hover:bg-white/25 transition-all duration-150">
        {marks.map((mark) => (
          <div
            key={`${mark.kind}-${mark.left}`}
            className={`absolute top-0 bottom-0 rounded-sm ${MARK_COLORS[mark.kind]}`}
            style={{ left: `${mark.left}%`, width: `${mark.width}%` }}
            aria-hidden="true"
          />
        ))}
        <div
          className="absolute left-0 top-0 bottom-0 rounded-full bg-gradient-to-r from-accent to-purple-400 shadow-sm"
          style={{ width: `${percent}%` }}
        />
        {hover && (
          <div
            className="absolute top-0 bottom-0 w-0.5 -ml-px bg-white/80"
            style={{ left: `${(hover.positionMs / durationMs) * 100}%` }}
            aria-hidden="true"
          />
        )}
        <div
          className="absolute top-1/2 -translate-y-1/2 -ml-2 w-4 h-4 rounded-full bg-white shadow-lg glow-accent opacity-0 group-hover:opacity-100 transition-opacity"
          style={{ left: `${percent}%` }}
        />
      </div>
    </div>
  );
};
