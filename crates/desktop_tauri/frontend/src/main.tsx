import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { installGlobalErrorLogging } from './utils/logger';
import './index.css';

installGlobalErrorLogging();

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
