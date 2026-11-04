import React from 'react';
import { SkipPrompt } from '../playback/skipPrompt';
import { createPromptWindow, promptKey, skipButtonVisible } from '../playback/skipWindow';

/**
 * Whether the skip button is on screen and whether its five seconds are still running. The button
 * shows by itself when its part of the video begins and goes after five seconds; after that it
 * shows only while the player's controls do.
 */
export function useSkipPromptWindow(
  prompt: SkipPrompt | null,
  controlsVisible: boolean,
): { visible: boolean; counting: boolean; key: string | null } {
  const [windowOpen, setWindowOpen] = React.useState(false);
  const [promptWindow] = React.useState(() => createPromptWindow(setWindowOpen));
  const key = promptKey(prompt);

  React.useEffect(() => promptWindow.show(key), [promptWindow, key]);
  React.useEffect(() => promptWindow.stop, [promptWindow]);

  const hasPrompt = prompt !== null;
  return {
    visible: skipButtonVisible({ hasPrompt, windowOpen, controlsVisible }),
    counting: hasPrompt && windowOpen,
    key,
  };
}
