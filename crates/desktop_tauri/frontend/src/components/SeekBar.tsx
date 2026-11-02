import React from 'react';
import { progressPercent, seekPositionFromClick } from '../utils/playerTime';

interface SeekBarProps {
  positionMs: number;
  durationMs: number;
  onSeek: (positionMs: number) => void;
}

/** The progress strip of the player; clicking it jumps to that point. */
export const SeekBar: React.FC<SeekBarProps> = ({ positionMs, durationMs, onSeek }) => {
  const percent = progressPercent(positionMs, durationMs);

  const handleClick = (event: React.MouseEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    onSeek(seekPositionFromClick(event.clientX, rect.left, rect.width, durationMs));
  };

  return (
    // The tall transparent padding makes the thin bar easy to hit on touch screens and small windows.
    <div onClick={handleClick} className="group py-2 cursor-pointer">
      <div className="relative h-1.5 sm:h-2 group-hover:h-2.5 rounded-full bg-white/20 group-hover:bg-white/25 transition-all duration-150">
        <div
          className="absolute left-0 top-0 bottom-0 rounded-full bg-gradient-to-r from-accent to-purple-400 shadow-sm"
          style={{ width: `${percent}%` }}
        />
        <div
          className="absolute top-1/2 -translate-y-1/2 -ml-2 w-4 h-4 rounded-full bg-white shadow-lg glow-accent opacity-0 group-hover:opacity-100 transition-opacity"
          style={{ left: `${percent}%` }}
        />
      </div>
    </div>
  );
};
