import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { installGlobalErrorLogging } from './utils/logger';
import { installImageLogging } from './utils/imageLog';
import { api } from './services/api';
import './index.css';

installGlobalErrorLogging();
installImageLogging((url, loaded) => {
  api.reportImage(url, loaded).catch(() => undefined);
});

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
