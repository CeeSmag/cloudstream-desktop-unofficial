import React from 'react';
import { SkipBack, SkipForward } from 'lucide-react';
import { SkipPrompt } from '../playback/skipPrompt';
import { WINDOW_MS } from '../playback/skipWindow';

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

/**
 * The button over the video for the segment being played (Skip Intro, Next episode, ...). While its
 * first seconds run (`counting`) a shade drains across it, so the viewer sees when it will go; it
 * fades out when not `visible`.
 */
export const SkipPromptButton: React.FC<{
  prompt: SkipPrompt;
  visible: boolean;
  counting: boolean;
  /** Changes with every new prompt, so the draining shade starts again from full. */
  drainKey: string | null;
  onClick: () => void;
}> = ({ prompt, visible, counting, drainKey, onClick }) => (
  <button
    onClick={onClick}
    tabIndex={visible ? 0 : -1}
    aria-hidden={!visible}
    className={`absolute right-4 sm:right-8 bottom-28 sm:bottom-32 z-10 flex items-center gap-2 px-4 py-2.5 rounded-xl overflow-hidden bg-white/95 hover:bg-white text-midnight text-sm font-semibold shadow-2xl transition-opacity duration-200 ${
      visible ? 'opacity-100' : 'opacity-0 pointer-events-none translate-y-1'
    }`}
  >
    {counting && (
      <span
        key={drainKey}
        className="skip-drain absolute inset-0 bg-black/25 pointer-events-none"
        style={{ animationDuration: `${WINDOW_MS}ms` }}
        aria-hidden="true"
      />
    )}
    <SkipForward className="relative w-4 h-4" />
    <span className="relative">{prompt.label}</span>
  </button>
);
