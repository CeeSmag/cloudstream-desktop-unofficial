import React from 'react';

/** How long a feedback pop-up stays on screen. */
export const FEEDBACK_MS = 900;

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

/** What a pop-up shows; `id` changes with every new one so it restarts its animation. */
export interface Feedback {
  id: number;
  icon: FeedbackIcon;
  text: string;
  /** 0 to 100: draws a level bar, used for the volume. */
  level?: number;
}

/** The pop-up in the middle of the player that confirms what the viewer just did. */
export function useFeedbackPopup() {
  const [popup, setPopup] = React.useState<Feedback | null>(null);
  const counter = React.useRef(0);
  const timer = React.useRef<number | null>(null);

  const show = React.useCallback((feedback: Omit<Feedback, 'id'>) => {
    counter.current += 1;
    setPopup({ ...feedback, id: counter.current });
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setPopup(null), FEEDBACK_MS);
  }, []);

  React.useEffect(
    () => () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    },
    [],
  );

  return { popup, show };
}
