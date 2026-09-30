import { createRoot } from 'react-dom/client';
import './style.css';
import { Architecture } from './Architecture';

const root = document.getElementById('root');
if (!root) throw new Error('Missing root');
createRoot(root).render(<Architecture onClose={() => window.location.assign('/')}/>);
