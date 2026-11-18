/** One entry of the app's own right-click menu. */
export interface ContextMenuItem {
  id: string;
  label: string;
  onSelect: () => void;
}

/** What is needed of the element that was clicked (a DOM element fits). */
export interface MenuTarget {
  tagName?: string;
  isContentEditable?: boolean;
}

/** Fields where the browser's own menu (cut, copy, paste) is still wanted. */
export function keepsNativeMenu(target: MenuTarget | null): boolean {
  if (!target) return false;
  if (target.isContentEditable) return true;
  const tag = (target.tagName ?? '').toUpperCase();
  return tag === 'INPUT' || tag === 'TEXTAREA';
}

/**
 * The entries of the app's own menu for what was clicked. There are none yet: the menu is a
 * place to add them later, and until then right-clicking shows nothing.
 */
export function menuItemsFor(_target: MenuTarget | null): ContextMenuItem[] {
  return [];
}
