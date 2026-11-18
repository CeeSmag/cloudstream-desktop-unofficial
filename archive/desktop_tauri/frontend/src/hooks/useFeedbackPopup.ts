import React from 'react';

/** How long a feedback pop-up stays fully visible after the last thing the viewer did. */
export const FEEDBACK_MS = 900;
/** How long the pop-up takes to fade away; it is removed when the fade has ended. */
export const FEEDBACK_FADE_MS = 300;

/** The picture shown in a pop-up. */
export type FeedbackIcon =
  | 'play'
  | 'pause'
  | 'volume'
  | 'mute'
  | 'forward'
  | 'back'
  | 'jump'
  | 'fullscreen'
  | 'windowed'
  | 'subtitles'
  | 'audio'
  | 'source';

/** What a pop-up shows. */
export interface Feedback {
  icon: FeedbackIcon;
  text: string;
  /** 0 to 100: draws a level bar, used for the volume. */
  level?: number;
}

/**
 * The pop-up in the middle of the player that confirms what the viewer just did. While the viewer
 * keeps acting (turning the volume, say) one pop-up stays on screen and updates in place; it fades
 * out [`FEEDBACK_MS`] after the last action.
 */
export function useFeedbackPopup() {
  const [popup, setPopup] = React.useState<Feedback | null>(null);
  const [visible, setVisible] = React.useState(false);
  const fadeTimer = React.useRef<number | null>(null);
  const removeTimer = React.useRef<number | null>(null);

  const clearTimers = () => {
    if (fadeTimer.current !== null) window.clearTimeout(fadeTimer.current);
    if (removeTimer.current !== null) window.clearTimeout(removeTimer.current);
  };

  const show = React.useCallback((feedback: Feedback) => {
    clearTimers();
    setPopup(feedback);
    setVisible(true);
    fadeTimer.current = window.setTimeout(() => setVisible(false), FEEDBACK_MS);
    removeTimer.current = window.setTimeout(() => setPopup(null), FEEDBACK_MS + FEEDBACK_FADE_MS);
  }, []);

  React.useEffect(() => clearTimers, []);

  return { popup, visible, show };
}
