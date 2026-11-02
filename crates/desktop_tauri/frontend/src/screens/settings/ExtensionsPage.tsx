import React from 'react';
import { Package, Globe, Store } from 'lucide-react';
import { useExtensions } from '../../hooks/useExtensions';
import { InstalledExtensionList } from '../../components/extensions/InstalledExtensionList';
import { RepositoryPanel } from '../../components/extensions/RepositoryPanel';
import { PluginList } from '../../components/extensions/PluginList';

interface ExtensionsPageProps {
  /** Shows a failure to the user. */
  onError: (error: unknown) => void;
}

const SECTION = 'p-4 sm:p-6 rounded-2xl glass-panel space-y-4';

function SectionTitle({ icon, title, hint }: { icon: React.ReactNode; title: string; hint: string }) {
  return (
    <div>
      <h2 className="text-lg font-bold text-white flex items-center gap-2">
        {icon}
        <span>{title}</span>
      </h2>
      <p className="text-xs text-zinc-400 mt-1">{hint}</p>
    </div>
  );
}

/** Everything about extensions: what is installed, which repositories are known, and what they offer. */
export const ExtensionsPage: React.FC<ExtensionsPageProps> = ({ onError }) => {
  const extensions = useExtensions(onError);
  const browsedName = extensions.repositories.find((repository) => repository.url === extensions.browsing)?.name;

  return (
    <div className="space-y-8 sm:space-y-10">
      <div className={SECTION}>
        <SectionTitle
          icon={<Package className="w-5 h-5 text-accent" />}
          title={`Installed (${extensions.installed.length})`}
          hint="Extensions run in a sandbox. Trust one only if you know its author."
        />
        <InstalledExtensionList
          extensions={extensions.installed}
          busy={extensions.busy}
          onUninstall={extensions.uninstall}
          onSetTrusted={extensions.setTrusted}
        />
      </div>

      <div className={SECTION}>
        <SectionTitle
          icon={<Globe className="w-5 h-5 text-accent" />}
          title={`Repositories (${extensions.repositories.length})`}
          hint="Add the address of a repository, then pick it to see the extensions it offers."
        />
        <RepositoryPanel
          repositories={extensions.repositories}
          browsing={extensions.browsing}
          busy={extensions.busy}
          onAdd={extensions.addRepository}
          onRemove={extensions.removeRepository}
          onBrowse={extensions.browse}
        />
      </div>

      {extensions.browsing && (
        <div className={SECTION}>
          <SectionTitle
            icon={<Store className="w-5 h-5 text-accent" />}
            title={browsedName ?? 'Extensions'}
            hint="Installing downloads the extension and loads it right away."
          />
          <PluginList
            plugins={extensions.plugins}
            loading={extensions.pluginsLoading}
            busy={extensions.busy}
            onInstall={(internalName) => {
              if (extensions.browsing) void extensions.install(extensions.browsing, internalName);
            }}
          />
        </div>
      )}
    </div>
  );
};
