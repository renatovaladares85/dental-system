import { createRoot } from 'react-dom/client';

import { App } from './app/App';
import { registerServiceWorker } from './pwa/registerServiceWorker';
import './styles/index.css';

const rootElement = document.getElementById('root');

if (!rootElement) {
  throw new Error('Elemento raiz da aplicação não encontrado.');
}

createRoot(rootElement).render(<App />);

registerServiceWorker();
