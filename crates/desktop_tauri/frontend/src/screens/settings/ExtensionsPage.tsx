import React from 'react';
import { Package, Globe } from 'lucide-react';
import type { ExtensionsState } from '../../hooks/useExtensions';
import { InstalledExtensionList } from '../../components/extensions/InstalledExtensionList';
import { RepositoryPanel } from '../../components/extensions/RepositoryPanel';

interface ExtensionsPageProps {
  extensions: ExtensionsState;
}

const SECTION = 'p-4 sm:p-5 rounded-2xl glass-panel space-y-3';

function SectionTitle({ icon, title, hint }: { icon: React.ReactNode; title: string; hint: string }) {
  return (
    <div>
      <h2 className="text-base font-bold text-white flex items-center gap-2">
        {icon}
        <span>{title}</span>
      </h2>
      <p className="text-[11px] text-zinc-500 mt-0.5">{hint}</p>
    </div>
  );
}

/** What is installed and which repositories are known; a repository opens on a page of its own. */
export const ExtensionsPage: React.FC<ExtensionsPageProps> = ({ extensions }) => (
  <div className="space-y-5">
    <div className={SECTION}>
      <SectionTitle
        icon={<Package className="w-4 h-4 text-accent" />}
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
        icon={<Globe className="w-4 h-4 text-accent" />}
        title={`Repositories (${extensions.repositories.length})`}
        hint="Open a repository to see all the extensions it offers."
      />
      <RepositoryPanel
        repositories={extensions.repositories}
        busy={extensions.busy}
        onAdd={extensions.addRepository}
        onRemove={extensions.removeRepository}
        onOpen={extensions.browse}
      />
    </div>
  </div>
);
