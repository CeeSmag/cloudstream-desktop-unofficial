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
}

/** A short pop-up in the middle of the player that confirms the last thing the viewer did. */
export const FeedbackPopup: React.FC<FeedbackPopupProps> = ({ feedback }) => {
  if (!feedback) return null;
  const Icon = ICONS[feedback.icon];
  return (
    <div className="pointer-events-none fixed inset-0 z-[60] flex items-center justify-center px-6" aria-live="polite">
      <div
        key={feedback.id}
        role="status"
        className="feedback-pop flex max-w-[80vw] items-center gap-3 rounded-2xl border border-white/10 bg-black/75 px-5 py-3 text-white shadow-2xl backdrop-blur-md"
      >
        <Icon className="h-7 w-7 flex-none text-accent" />
        <span className="break-words text-lg font-semibold">{feedback.text}</span>
        {feedback.level !== undefined && (
          <span className="h-1.5 w-28 flex-none overflow-hidden rounded-full bg-white/20">
            <span className="block h-full rounded-full bg-accent" style={{ width: `${feedback.level}%` }} />
          </span>
        )}
      </div>
    </div>
  );
};
