import React from 'react';
import {
  AudioLines,
  FastForward,
  Layers,
  Maximize2,
  Minimize2,
  Pause,
  Play,
  Rewind,
  SkipForward,
  Subtitles,
  Volume2,
  VolumeX,
} from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import type { Feedback, FeedbackIcon } from '../hooks/useFeedbackPopup';

const ICONS: Record<FeedbackIcon, LucideIcon> = {
  play: Play,
  pause: Pause,
  volume: Volume2,
  mute: VolumeX,
  forward: FastForward,
  back: Rewind,
  jump: SkipForward,
  fullscreen: Maximize2,
  windowed: Minimize2,
  subtitles: Subtitles,
  audio: AudioLines,
  source: Layers,
};

interface FeedbackPopupProps {
  feedback: Feedback | null;
  /** False while the pop-up fades out. */
  visible: boolean;
}

/**
 * A see-through pop-up in the middle of the player that confirms the last thing the viewer did.
 * It appears once and then updates in place, so quick repeated changes do not make it flicker.
 */
export const FeedbackPopup: React.FC<FeedbackPopupProps> = ({ feedback, visible }) => {
  if (!feedback) return null;
  const Icon = ICONS[feedback.icon];
  return (
    <div className="pointer-events-none fixed inset-0 z-[60] flex items-center justify-center px-6" aria-live="polite">
      <div
        role="status"
        className={`feedback-in flex max-w-[80vw] items-center gap-3 rounded-2xl border border-white/10 bg-black/25 px-5 py-3 text-white transition-opacity duration-300 ${
          visible ? 'opacity-100' : 'opacity-0'
        }`}
      >
        <Icon className="h-7 w-7 flex-none text-accent drop-shadow-[0_1px_3px_rgba(0,0,0,0.9)]" />
        <span className="break-words text-lg font-semibold drop-shadow-[0_1px_3px_rgba(0,0,0,0.9)]">{feedback.text}</span>
        {feedback.level !== undefined && (
          <span className="h-1.5 w-28 flex-none overflow-hidden rounded-full bg-white/25">
            <span
              className="block h-full rounded-full bg-accent transition-[width] duration-100"
              style={{ width: `${feedback.level}%` }}
            />
          </span>
        )}
      </div>
    </div>
  );
};
