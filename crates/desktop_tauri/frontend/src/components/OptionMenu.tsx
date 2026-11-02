import React from 'react';
import type { LucideIcon } from 'lucide-react';

/** One row of a player dropdown. */
export interface MenuOption {
  id: number;
  label: string;
  selected: boolean;
}

interface OptionMenuProps {
  /** Heading of the dropdown and tooltip of the button. */
  label: string;
  icon: LucideIcon;
  options: MenuOption[];
  /** Adds an "Off" entry, used for subtitles. */
  allowOff: boolean;
  open: boolean;
  onToggle: () => void;
  onSelect: (id: number | null) => void;
}

const ITEM_BASE = 'w-full text-left px-3 py-2 text-xs rounded-lg flex items-start justify-between gap-3 transition-colors';

/** A player button that opens a dropdown of choices; long names wrap onto the next line. */
export const OptionMenu: React.FC<OptionMenuProps> = ({
  label,
  icon: Icon,
  options,
  allowOff,
  open,
  onToggle,
  onSelect,
}) => (
  <div className="relative">
    <button
      onClick={onToggle}
      className={`p-2 sm:p-2.5 rounded-xl border transition-colors backdrop-blur-md ${
        open ? 'bg-accent border-accent text-white' : 'bg-white/10 hover:bg-white/20 border-white/10 text-white'
      }`}
      title={label}
      aria-label={label}
    >
      <Icon className="w-5 h-5" />
    </button>

    {open && (
      <div className="absolute right-0 mt-2 w-72 sm:w-80 max-w-[calc(100vw-2rem)] max-h-[60vh] overflow-y-auto rounded-xl glass-dropdown p-2 shadow-2xl z-50">
        <div className="px-3 py-1 text-[11px] font-semibold text-zinc-400 uppercase tracking-wider">{label}</div>
        {allowOff && (
          <button onClick={() => onSelect(null)} className={`${ITEM_BASE} text-zinc-300 hover:bg-white/5`}>
            Off
          </button>
        )}
        {options.length === 0 && !allowOff && (
          <div className="px-3 py-2 text-xs text-zinc-500">Nothing to choose from.</div>
        )}
        {options.map((option) => (
          <button
            key={option.id}
            onClick={() => onSelect(option.id)}
            className={`${ITEM_BASE} ${
              option.selected ? 'bg-accent/20 text-accent font-semibold' : 'text-zinc-300 hover:bg-white/5'
            }`}
          >
            <span className="whitespace-normal break-words min-w-0">{option.label}</span>
            {option.selected && <span className="mt-1 w-1.5 h-1.5 flex-none rounded-full bg-accent" />}
          </button>
        ))}
      </div>
    )}
  </div>
);
