import React from 'react';
import { ContextMenuItem, keepsNativeMenu, menuItemsFor } from '../utils/contextMenu';

/** An open menu: where it is and what it offers. */
export interface OpenContextMenu {
  x: number;
  y: number;
  items: ContextMenuItem[];
}

/**
 * Replaces the browser's right-click menu (Back, Refresh, Save as, Inspect…) with the app's own,
 * which has no entries yet, so for now a right-click shows nothing. Text fields keep the browser's
 * menu so cut, copy and paste still work there.
 */
export function useContextMenu() {
  const [menu, setMenu] = React.useState<OpenContextMenu | null>(null);

  React.useEffect(() => {
    const close = () => setMenu(null);
    const onContextMenu = (event: MouseEvent) => {
      if (keepsNativeMenu(event.target as HTMLElement | null)) return;
      event.preventDefault();
      const items = menuItemsFor(event.target as HTMLElement | null);
      setMenu(items.length > 0 ? { x: event.clientX, y: event.clientY, items } : null);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') close();
    };
    document.addEventListener('contextmenu', onContextMenu);
    window.addEventListener('click', close);
    window.addEventListener('blur', close);
    window.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('contextmenu', onContextMenu);
      window.removeEventListener('click', close);
      window.removeEventListener('blur', close);
      window.removeEventListener('keydown', onKey);
    };
  }, []);

  return { menu, close: () => setMenu(null) };
}
