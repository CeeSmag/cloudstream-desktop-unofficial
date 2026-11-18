import React from 'react';
import { Search } from 'lucide-react';
import type { ExtensionsState } from '../../hooks/useExtensions';
import { useAllPlugins } from '../../hooks/useAllPlugins';
import { PluginEntry, PluginList } from '../../components/extensions/PluginList';
import { api } from '../../services/api';
import { PluginView } from '../../utils/extensionText';
import { filterHits, languageChoices } from '../../utils/pluginCatalog';
import { PluginSort, sortHits } from '../../utils/pluginSort';

interface PluginsTabProps {
  extensions: ExtensionsState;
  onError: (error: unknown) => void;
}

const VIEWS: { id: PluginView; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'available', label: 'Not installed' },
  { id: 'installed', label: 'Installed' },
  { id: 'updates', label: 'Updates' },
];

/** How many cards are drawn at first, and how many more each "Show more" adds. */
const PAGE = 60;

/** Every extension of every repository in one place: search it, filter it, install from it. */
export const PluginsTab: React.FC<PluginsTabProps> = ({ extensions, onError }) => {
  const { hits, loading, reload } = useAllPlugins(onError);
  const [text, setText] = React.useState('');
  const [view, setView] = React.useState<PluginView>('all');
  const [language, setLanguage] = React.useState('');
  const [sort, setSort] = React.useState<PluginSort>('name');
  const [preferredSubtitles, setPreferredSubtitles] = React.useState<string[]>([]);
  const [shown, setShown] = React.useState(PAGE);

  React.useEffect(() => {
    api
      .getPreferences()
      .then((preferences) => setPreferredSubtitles(preferences.preferred_subtitles))
      .catch(onError);
  }, [onError]);

  const matching = React.useMemo(() => {
    const filtered = filterHits(hits, { text, view, language });
    return sortHits(filtered, sort, preferredSubtitles);
  }, [hits, text, view, language, sort, preferredSubtitles]);
  const languages = React.useMemo(
    () => languageChoices(hits, preferredSubtitles),
    [hits, preferredSubtitles],
  );
  const entries: PluginEntry[] = React.useMemo(
    () =>
      matching.slice(0, shown).map((hit) => ({
        key: `${hit.repository_url}#${hit.plugin.internal_name}`,
        plugin: hit.plugin,
        repository: { name: hit.repository_name, url: hit.repository_url },
      })),
    [matching, shown],
  );

  // A new search, filter or sort starts again from the top of the list.
  React.useEffect(() => setShown(PAGE), [text, view, language, sort]);

  const install = async (entry: PluginEntry) => {
    if (entry.repository && (await extensions.install(entry.repository.url, entry.plugin.internal_name))) await reload();
  };

  const filtering = text.trim() !== '' || view !== 'all' || language !== '';

  return (
    <div className="space-y-4">
      <div className="flex flex-col lg:flex-row lg:items-center gap-3">
        <div className="relative flex-1 min-w-0">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500" />
          <input
            type="text"
            value={text}
            onChange={(event) => setText(event.target.value)}
            placeholder="Search extensions by name, description, author, language or repository"
            className="w-full pl-9 pr-3 py-2.5 rounded-xl bg-surface-elevated border border-white/10 focus:border-accent text-white placeholder-zinc-500 text-xs focus:outline-none"
          />
        </div>
        <select
          value={language}
          onChange={(event) => setLanguage(event.target.value)}
          aria-label="Language"
          className="rounded-xl bg-surface-elevated border border-white/10 px-3 py-2.5 text-xs text-white"
        >
          <option value="">All languages</option>
          {languages.map((entry) => (
            <option key={entry.code} value={entry.code}>
              {entry.code.toUpperCase()} ({entry.count})
            </option>
          ))}
        </select>
        <select
          value={sort}
          onChange={(event) => setSort(event.target.value as PluginSort)}
          aria-label="Sort plugins"
          className="rounded-xl bg-surface-elevated border border-white/10 px-3 py-2.5 text-xs text-white"
        >
          <option value="name">Sort: Name</option>
          <option value="language">Sort: Language</option>
          <option value="size">Sort: Size</option>
          <option value="date">Sort: Date added</option>
        </select>
        <div className="flex flex-wrap items-center gap-1.5 p-1 rounded-xl bg-surface-elevated border border-white/5">
          {VIEWS.map((entry) => (
            <button
              key={entry.id}
              onClick={() => setView(entry.id)}
              className={`px-3 py-1.5 rounded-lg text-xs font-medium transition-colors ${
                view === entry.id ? 'bg-accent text-white font-semibold shadow-sm' : 'text-zinc-400 hover:text-white'
              }`}
            >
              {entry.label}
            </button>
          ))}
        </div>
      </div>

      {loading ? (
        <p className="text-xs text-zinc-500">Downloading the extension lists of all repositories. The first time can take several seconds.</p>
      ) : (
        <p className="text-xs text-zinc-400">
          {matching.length} of {hits.length} extensions
        </p>
      )}

      <PluginList
        entries={entries}
        loading={loading}
        busy={extensions.busy}
        onInstall={(entry) => void install(entry)}
        onOpenRepository={extensions.browse}
        emptyText={filtering ? 'No extension matches that search or filter.' : 'The repositories do not offer any extensions.'}
      />

      {!loading && matching.length > shown && (
        <button
          onClick={() => setShown(shown + PAGE)}
          className="w-full py-2.5 rounded-xl bg-white/10 hover:bg-white/15 text-white text-xs font-medium transition-colors"
        >
          Show {Math.min(PAGE, matching.length - shown)} more ({matching.length - shown} left)
        </button>
      )}
    </div>
  );
};
