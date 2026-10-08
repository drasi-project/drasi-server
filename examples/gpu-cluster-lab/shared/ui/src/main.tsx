import React, { lazy, Suspense } from 'react';
import { createRoot } from 'react-dom/client';
import { NativeSseProvider } from '@drasi/example-native-sse';
import { LiveApp } from './App';
import { queryIds } from './rows';
import '@drasi/react/styles.css';
import './style.css';

const MockApp = import.meta.env.DEV && import.meta.env.MODE === 'mock'
  ? lazy(() => import('./mock/MockApp')) : null;
const root = document.getElementById('root');
if (!root) throw new Error('Missing application root');
createRoot(root).render(<React.StrictMode>{MockApp
  ? <Suspense fallback={<p>Loading mock UI preview…</p>}><MockApp/></Suspense>
  : <NativeSseProvider origin={location.origin} instanceId="gpu-demo" queryIds={queryIds}
      sinkId="gpu-demo-ui" endpoint={`${location.origin}/events/gpu-demo`}><LiveApp/></NativeSseProvider>
}</React.StrictMode>);
