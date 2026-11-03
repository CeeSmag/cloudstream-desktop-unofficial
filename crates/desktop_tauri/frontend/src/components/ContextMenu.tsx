import React from 'react';
import { OpenContextMenu } from '../hooks/useContextMenu';

interface ContextMenuProps {
  menu: OpenContextMenu | null;
  onClose: () => void;
}

/** The app's own right-click menu. Shows nothing while it has no entries. */
export const ContextMenu: React.FC<ContextMenuProps> = ({ menu, onClose }) => {
  if (!menu || menu.items.length === 0) return null;
  return (
    <div
      role="menu"
      style={{ left: menu.x, top: menu.y }}
      className="fixed z-[100] min-w-44 py-1.5 rounded-xl bg-surface-elevated border border-white/10 shadow-2xl"
      onContextMenu={(event) => event.preventDefault()}
    >
      {menu.items.map((item) => (
        <button
          key={item.id}
          role="menuitem"
          onClick={() => {
            onClose();
            item.onSelect();
          }}
          className="w-full text-left px-4 py-2 text-sm text-zinc-200 hover:bg-white/10"
        >
          {item.label}
        </button>
      ))}
    </div>
  );
};
