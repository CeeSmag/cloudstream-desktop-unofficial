import React from 'react';
import { SkipBack, SkipForward } from 'lucide-react';
import { SkipPrompt } from '../playback/skipPrompt';

const BUTTON =
  'p-2 sm:p-2.5 rounded-xl bg-white/10 hover:bg-white/20 text-white border border-white/10 transition-colors flex items-center gap-1.5';

interface EpisodeButtonsProps {
  /** Plays the episode before this one; absent for a movie or the first episode. */
  onPrevious?: () => void;
  /** Plays the episode after this one; absent for a movie or the last episode. */
  onNext?: () => void;
}

/** The Previous episode and Next episode buttons of the player's control bar. */
export const EpisodeButtons: React.FC<EpisodeButtonsProps> = ({ onPrevious, onNext }) => (
  <>
    {onPrevious && (
      <button onClick={onPrevious} className={BUTTON} title="Previous episode">
        <SkipBack className="w-4 h-4" />
        <span className="hidden lg:inline text-xs font-medium">Previous episode</span>
      </button>
    )}
    {onNext && (
      <button onClick={onNext} className={BUTTON} title="Next episode">
        <SkipForward className="w-4 h-4" />
        <span className="hidden lg:inline text-xs font-medium">Next episode</span>
      </button>
    )}
  </>
);

/** The button over the video for the segment being played (Skip Intro, Next episode, ...). */
export const SkipPromptButton: React.FC<{ prompt: SkipPrompt; onClick: () => void }> = ({ prompt, onClick }) => (
  <button
    onClick={onClick}
    className="absolute right-4 sm:right-8 bottom-28 sm:bottom-32 z-10 flex items-center gap-2 px-4 py-2.5 rounded-xl bg-white/90 hover:bg-white text-midnight text-sm font-semibold shadow-2xl backdrop-blur-md transition-colors"
  >
    <SkipForward className="w-4 h-4" />
    {prompt.label}
  </button>
);
