import React from 'react';
import type { LucideIcon } from 'lucide-react';

/** One row of a player dropdown. */
export interface MenuOption {
  id: number;
  label: string;
  selected: boolean;
  /** Small tags under the name, such as the kind of stream and its host. */
  tags?: string[];
  /** A flag on the right, such as the size of the file. */
  badge?: string;
  /** Longer text under the tags, shown in full and wrapped, such as the stream address. */
  detail?: string;
}

interface OptionMenuProps {
  /** Heading of the dropdown and tooltip of the button. */
  label: string;
  icon: LucideIcon;
  options: MenuOption[];
  /** Extra content under the heading, such as quick switches. */
  header?: React.ReactNode;
  /** Adds an "Off" entry, used for subtitles. */
  allowOff: boolean;
  /** Makes the dropdown wider, for rows that carry details. */
  wide?: boolean;
  open: boolean;
  onToggle: () => void;
  onSelect: (id: number | null) => void;
}

const ITEM_BASE = 'w-full text-left px-3 py-2 text-xs rounded-lg flex items-start justify-between gap-3 transition-colors';

/** A player button that opens a dropdown of choices; long names and details wrap onto the next line. */
export const OptionMenu: React.FC<OptionMenuProps> = ({
  label,
  icon: Icon,
  options,
  header,
  allowOff,
  wide = false,
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
      <div
        data-wheel-scroll
        className={`absolute right-0 mt-2 ${
          wide ? 'w-[26rem]' : 'w-72 sm:w-80'
        } max-w-[calc(100vw-2rem)] max-h-[60vh] overflow-y-auto rounded-xl glass-dropdown p-2 shadow-2xl z-50`}
      >
        <div className="px-3 py-1 text-[11px] font-semibold text-zinc-400 uppercase tracking-wider">{label}</div>
        {header}
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
            <span className="min-w-0 flex-1 space-y-1">
              <span className="block whitespace-normal break-words">{option.label}</span>
              {option.tags && option.tags.length > 0 && (
                <span className="flex flex-wrap gap-1">
                  {option.tags.map((tag) => (
                    <span key={tag} className="px-1.5 py-0.5 rounded bg-white/10 text-[10px] font-medium text-zinc-300 break-all">
                      {tag}
                    </span>
                  ))}
                </span>
              )}
              {option.detail && (
                <span className="block font-mono text-[10px] font-normal text-zinc-500 break-all">{option.detail}</span>
              )}
            </span>
            <span className="flex flex-none items-center gap-2">
              {option.badge && (
                <span className="px-1.5 py-0.5 rounded border border-accent/40 bg-accent/10 text-[10px] font-semibold text-accent whitespace-nowrap">
                  {option.badge}
                </span>
              )}
              {option.selected && <span className="w-1.5 h-1.5 flex-none rounded-full bg-accent" />}
            </span>
          </button>
        ))}
      </div>
    )}
  </div>
);
