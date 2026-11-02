import React from 'react';
import { Home, Search, Bookmark, Settings, ChevronDown, Film } from 'lucide-react';
import { ProviderInfo } from '../types';

interface NavigationProps {
  currentTab: string;
  onSelectTab: (tab: string) => void;
  providers: ProviderInfo[];
  selectedProvider: string;
  onSelectProvider: (provider: string) => void;
}

export const Navigation: React.FC<NavigationProps> = ({
  currentTab,
  onSelectTab,
  providers,
  selectedProvider,
  onSelectProvider,
}) => {
  const [providerOpen, setProviderOpen] = React.useState(false);

  const navItems = [
    { id: 'home', label: 'Home', icon: Home },
    { id: 'search', label: 'Search', icon: Search },
    { id: 'library', label: 'Library', icon: Bookmark },
    { id: 'settings', label: 'Settings', icon: Settings },
  ];

  return (
    <header className="h-14 sm:h-16 page-gutter gap-2 glass-panel border-b border-white/5 flex items-center justify-between z-40 select-none">
      {/* Brand Logo */}
      <div className="flex flex-none items-center gap-3 cursor-pointer" onClick={() => onSelectTab('home')}>
        <div className="w-9 h-9 rounded-xl bg-gradient-to-tr from-accent to-purple-400 flex items-center justify-center glow-accent">
          <Film className="w-5 h-5 text-white" />
        </div>
        <div className="hidden md:flex flex-col">
          <span className="text-base font-bold tracking-tight text-white flex items-center gap-1.5">
            CloudStream <span className="text-xs px-1.5 py-0.5 rounded bg-accent/20 text-accent font-semibold">RS</span>
          </span>
        </div>
      </div>

      {/* Center Nav Links */}
      <nav className="flex items-center gap-0.5 sm:gap-1">
        {navItems.map((item) => {
          const Icon = item.icon;
          const isActive = currentTab === item.id;
          return (
            <button
              key={item.id}
              onClick={() => onSelectTab(item.id)}
              title={item.label}
              aria-label={item.label}
              className={`flex items-center gap-2 px-2.5 sm:px-3 lg:px-4 py-2 rounded-xl text-sm font-medium transition-all duration-200 ${
                isActive
                  ? 'bg-accent/15 text-accent shadow-sm border border-accent/20 font-semibold'
                  : 'text-zinc-400 hover:text-white hover:bg-white/5'
              }`}
            >
              <Icon className="w-4 h-4" />
              <span className="hidden lg:inline">{item.label}</span>
            </button>
          );
        })}
      </nav>

      {/* Right Controls: Provider Pill */}
      <div className="relative flex-none">
        <button
          onClick={() => setProviderOpen(!providerOpen)}
          className="flex items-center space-x-2 px-3.5 py-1.5 rounded-xl bg-white/5 hover:bg-white/10 border border-white/10 text-xs font-medium text-zinc-300 transition-colors"
        >
          <span className="w-2 h-2 rounded-full bg-emerald-400"></span>
          <span className="truncate max-w-[5rem] sm:max-w-[9rem]">{selectedProvider || 'Select Provider'}</span>
          <ChevronDown className="w-3.5 h-3.5 text-zinc-400" />
        </button>

        {providerOpen && (
          <div className="absolute right-0 mt-2 w-48 max-h-[70vh] overflow-y-auto py-1 rounded-xl glass-dropdown shadow-2xl z-50 animate-in fade-in zoom-in-95 duration-100">
            <div className="px-3 py-1.5 text-[10px] font-semibold text-zinc-400 uppercase tracking-wider">
              Active Provider
            </div>
            {providers.map((p) => (
              <button
                key={p.name}
                onClick={() => {
                  onSelectProvider(p.name);
                  setProviderOpen(false);
                }}
                className={`w-full text-left px-3 py-2 text-xs flex items-center justify-between transition-colors ${
                  selectedProvider === p.name
                    ? 'text-accent bg-accent/10 font-medium'
                    : 'text-zinc-300 hover:bg-white/5'
                }`}
              >
                <span>{p.name}</span>
                {selectedProvider === p.name && <span className="w-1.5 h-1.5 rounded-full bg-accent"></span>}
              </button>
            ))}
          </div>
        )}
      </div>
    </header>
  );
};
