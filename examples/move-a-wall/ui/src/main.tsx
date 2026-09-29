import React from 'react';
import { createRoot } from 'react-dom/client';
import { DrasiProvider } from '@drasi/react/react';
import { sse034ResultAdapter } from '@drasi/react/client';
import { queryIds } from './records';
import { App } from './App';
import '@drasi/react/styles.css';
import './style.css';
const root = document.getElementById('root');
if (!root) throw new Error('Missing root');
createRoot(root).render(<React.StrictMode>
  <DrasiProvider serverUrl={location.origin} instanceId="move-a-wall" queryIds={[...queryIds]}
    reaction={{ id:'wall-ui',endpoint:`${location.origin}/events` }} resultAdapter={sse034ResultAdapter}>
    <App/>
  </DrasiProvider>
</React.StrictMode>);
