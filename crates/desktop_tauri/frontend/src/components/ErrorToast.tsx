import React from 'react';
import { AlertTriangle, X } from 'lucide-react';

interface ErrorToastProps {
  message: string | null;
  onDismiss: () => void;
}

/** A dismissible error message pinned to the top of the window. */
export const ErrorToast: React.FC<ErrorToastProps> = ({ message, onDismiss }) => {
  if (!message) return null;
  return (
    <div
      role="alert"
      className="fixed top-4 left-1/2 -translate-x-1/2 z-50 flex max-w-xl items-start space-x-3 rounded-xl border border-red-500/30 bg-red-950/95 px-4 py-3 text-sm text-red-100 shadow-xl"
    >
      <AlertTriangle className="mt-0.5 h-4 w-4 flex-none text-red-400" />
      <span className="flex-1">{message}</span>
      <button onClick={onDismiss} aria-label="Dismiss" className="text-red-300 hover:text-white">
        <X className="h-4 w-4" />
      </button>
    </div>
  );
};
