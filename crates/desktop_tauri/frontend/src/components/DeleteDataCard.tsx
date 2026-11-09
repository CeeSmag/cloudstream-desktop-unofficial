import React from 'react';
import { Trash2, AlertTriangle } from 'lucide-react';
import { api } from '../services/api';

interface DeleteDataCardProps {
  onError: (error: unknown) => void;
}

/** What goes, so nobody is surprised. */
const DELETED = [
  'Library (bookmarks) and Continue Watching',
  'Watch progress of every episode',
  'Installed plugins and added repositories',
  'Your settings (languages, playback options, hidden providers)',
  'Saved stream sources, subtitles and log files',
];

type Step = 'idle' | 'confirming' | 'deleting';

/** The last card of the General page: deletes everything the app saved, after a confirmation. */
export const DeleteDataCard: React.FC<DeleteDataCardProps> = ({ onError }) => {
  const [step, setStep] = React.useState<Step>('idle');

  const deleteEverything = async () => {
    setStep('deleting');
    try {
      await api.deleteAllData();
      // The page's own remembered choices go too; the app restarts right after.
      try {
        window.localStorage.clear();
      } catch {
        // Storage may be blocked; the saved data on disk is what matters.
      }
    } catch (error) {
      setStep('idle');
      onError(error);
    }
  };

  return (
    <div className="p-4 sm:p-6 rounded-2xl border border-red-500/25 bg-red-500/5 space-y-4">
      <div className="space-y-1">
        <h2 className="text-lg font-bold text-white flex items-center gap-2">
          <Trash2 className="w-5 h-5 text-red-400 flex-none" />
          <span>Delete all saved data</span>
        </h2>
        <p className="text-xs text-zinc-400">
          Puts CloudStream back to how it was on the first day. This cannot be undone.
        </p>
      </div>

      {step === 'idle' && (
        <button
          onClick={() => setStep('confirming')}
          className="inline-flex items-center gap-2 px-4 py-2 rounded-xl bg-red-500/15 hover:bg-red-500/25 border border-red-500/30 text-red-300 text-sm font-semibold transition-colors"
        >
          <Trash2 className="w-4 h-4" />
          Delete all saved data...
        </button>
      )}

      {step !== 'idle' && (
        <div className="space-y-3 rounded-xl bg-black/20 border border-white/10 p-4">
          <p className="flex items-center gap-2 text-sm font-semibold text-red-300">
            <AlertTriangle className="w-4 h-4 flex-none" />
            This will delete:
          </p>
          <ul className="list-disc pl-5 space-y-1 text-sm text-zinc-300">
            {DELETED.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
          <p className="text-xs text-zinc-400">CloudStream restarts afterwards, with no plugins installed.</p>
          <div className="flex flex-wrap gap-2 pt-1">
            <button
              onClick={() => void deleteEverything()}
              disabled={step === 'deleting'}
              className="px-4 py-2 rounded-xl bg-red-600 hover:bg-red-500 disabled:opacity-60 text-white text-sm font-semibold transition-colors"
            >
              {step === 'deleting' ? 'Deleting and restarting...' : 'Yes, delete everything'}
            </button>
            <button
              onClick={() => setStep('idle')}
              disabled={step === 'deleting'}
              className="px-4 py-2 rounded-xl bg-white/10 hover:bg-white/20 disabled:opacity-60 text-white text-sm font-medium transition-colors"
            >
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
};
