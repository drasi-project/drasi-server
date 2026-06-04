// Copyright 2025 The Drasi Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import React from 'react';
import ReactDOM from 'react-dom/client';
import { DrasiProvider } from '@drasi/react';
import App from './App';
import {
  DRASI_SERVER_URL,
  TRADING_QUERIES,
  TRADING_REACTION,
  routeTradingData,
} from '@/drasi/config';
import './index.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <DrasiProvider
      serverUrl={DRASI_SERVER_URL}
      queries={TRADING_QUERIES}
      reaction={TRADING_REACTION}
      routeUnidentified={routeTradingData}
    >
      <App />
    </DrasiProvider>
  </React.StrictMode>,
);