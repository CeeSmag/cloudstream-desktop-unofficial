/** Whether the controls fade out after a few still seconds: only while the video plays and no menu is open. */
export function shouldAutoHideControls(isPlaying: boolean, menuOpen: boolean): boolean {
  return isPlaying && !menuOpen;
}

/** What a click on the video itself does: with a menu open it only closes the menu, otherwise it plays or pauses. */
export function videoClickAction(menuOpen: boolean): 'close-menu' | 'toggle-play' {
  return menuOpen ? 'close-menu' : 'toggle-play';
}
