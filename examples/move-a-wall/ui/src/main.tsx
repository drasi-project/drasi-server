import React from 'react';
import { createRoot } from 'react-dom/client';
import { NativeSseProvider } from '@drasi/example-native-sse';
import { queryIds } from './records';
import { App } from './App';
import '@drasi/react/styles.css';
import './style.css';
const root = document.getElementById('root');
if (!root) throw new Error('Missing root');
createRoot(root).render(<React.StrictMode>
  <NativeSseProvider origin={location.origin} instanceId="move-a-wall" queryIds={queryIds}
    sinkId="wall-ui" endpoint={`${location.origin}/events`}>
    <App/>
  </NativeSseProvider>
</React.StrictMode>);
