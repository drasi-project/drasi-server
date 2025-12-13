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

import { StockList } from '@/components/StockList';
import { Portfolio } from '@/components/Portfolio';
import StockTicker from '@/components/StockTicker';
import { useConnectionStatus } from '@drasi/react';
import clsx from 'clsx';

function App() {
  const connectionStatus = useConnectionStatus();

  return (
    <div className="min-h-screen bg-trading-bg">
      {/* Header */}
      <header className="bg-trading-card border-b border-trading-border px-6 py-4">
        <div className="max-w-7xl mx-auto flex justify-between items-center">
          <h1 className="text-2xl font-bold text-white">
            Drasi Trading Demo
          </h1>
          <div className="flex items-center gap-4">
            <span className="text-sm text-gray-400">
              Powered by Drasi Server
            </span>
            <div className="flex items-center gap-2">
              <div className={clsx(
                "w-2 h-2 rounded-full animate-pulse",
                connectionStatus.connected ? "bg-trading-green" : "bg-trading-red"
              )} />
              <span className="text-sm text-gray-400">
                {connectionStatus.connected ? 'Connected' : 
                 connectionStatus.reconnecting ? 'Reconnecting...' : 'Disconnected'}
              </span>
            </div>
          </div>
        </div>
      </header>

      {/* Main Content */}
      <main className="max-w-7xl mx-auto p-6">
        <div className="grid grid-cols-1 lg:grid-cols-2 xl:grid-cols-3 gap-6">
          {/* Watchlist */}
          <div className="xl:col-span-1">
            <StockList 
              title="Watchlist" 
              queryId="watchlist-query"
              filter={(stock) => ['AAPL', 'MSFT', 'GOOGL', 'TSLA', 'NVDA'].includes(stock.symbol)}
              sortBy={(a, b) => {
                const watchlist = ['AAPL', 'MSFT', 'GOOGL', 'TSLA', 'NVDA'];
                return watchlist.indexOf(a.symbol) - watchlist.indexOf(b.symbol);
              }}
            />
          </div>

          {/* Portfolio */}
          <div className="xl:col-span-2">
            <Portfolio />
          </div>

          {/* Market Movers */}
          <div className="lg:col-span-1">
            <StockList 
              title="Top Gainers" 
              queryId="top-gainers-query"
              filter={(stock) => stock.changePercent > 0}
              sortBy={(a, b) => b.changePercent - a.changePercent}
            />
          </div>

          <div className="lg:col-span-1">
            <StockList 
              title="Top Losers" 
              queryId="top-losers-query"
              filter={(stock) => stock.changePercent < 0}
              sortBy={(a, b) => a.changePercent - b.changePercent}
            />
          </div>

          <div className="lg:col-span-1">
            <StockList 
              title="High Volume" 
              queryId="high-volume-query"
              sortBy={(a, b) => (b.volume || 0) - (a.volume || 0)}
              showVolume={true}
            />
          </div>
        </div>
      </main>

      {/* Footer */}
      <footer className="mt-12 border-t border-trading-border px-6 py-4 pb-16">
        <div className="max-w-7xl mx-auto text-center text-sm text-gray-500">
          <p>
            This is a demonstration of Drasi Server's continuous query capabilities.
            All data is simulated for demonstration purposes.
          </p>
        </div>
      </footer>

      {/* Stock Ticker */}
      <StockTicker />
    </div>
  );
}

export default App;