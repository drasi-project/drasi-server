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

import { useState, useEffect } from 'react';
import {
  Database,
  Code,
  Activity,
  AlertCircle,
  CheckCircle2,
  XCircle,
  Loader2
} from 'lucide-react';
import { SourceManager } from './components/SourceManager';
import { QueryManager } from './components/QueryManager';
import { QueryResults } from './components/QueryResults';
import { DataTable } from './components/DataTable';
import { useDrasiClient, useConnectionStatus, useSources, useQueries } from './hooks/useDrasi';

function App() {
  const [activeTab, setActiveTab] = useState<'sources' | 'data' | 'queries'>('sources');
  const [selectedSourceId, setSelectedSourceId] = useState<string | null>(null);
  const [selectedQueryId, setSelectedQueryId] = useState<string | null>(null);

  // Initialize Drasi client
  const { client, initialized, error: clientError } = useDrasiClient();
  const connectionStatus = useConnectionStatus();
  const { sources, loading: sourcesLoading } = useSources();
  const { queries } = useQueries();

  // Auto-select first source when sources load
  useEffect(() => {
    if (sources.length > 0 && !selectedSourceId) {
      setSelectedSourceId(sources[0].id);
    }
  }, [sources, selectedSourceId]);

  // Auto-select first query when queries load
  useEffect(() => {
    if (queries.length > 0 && !selectedQueryId) {
      setSelectedQueryId(queries[0].id);
    }
  }, [queries, selectedQueryId]);

  // Connection status indicator
  const getConnectionStatusIcon = () => {
    if (connectionStatus.connected) {
      return <CheckCircle2 className="w-4 h-4 text-green-500" />;
    } else if (connectionStatus.reconnecting) {
      return <Loader2 className="w-4 h-4 text-yellow-500 animate-spin" />;
    } else if (connectionStatus.error) {
      return <XCircle className="w-4 h-4 text-red-500" />;
    } else {
      return <AlertCircle className="w-4 h-4 text-gray-400" />;
    }
  };

  const getConnectionStatusText = () => {
    if (connectionStatus.connected) {
      return 'Connected';
    } else if (connectionStatus.reconnecting) {
      return 'Reconnecting...';
    } else if (connectionStatus.error) {
      return 'Disconnected';
    } else {
      return 'Connecting...';
    }
  };

  // Show loading state while initializing
  if (!initialized) {
    return (
      <div className="min-h-screen bg-gray-50 flex items-center justify-center">
        <div className="text-center">
          <Loader2 className="w-12 h-12 text-blue-500 animate-spin mx-auto mb-4" />
          <h2 className="text-xl font-semibold text-gray-900 mb-2">Initializing Drasi Playground</h2>
          <p className="text-sm text-gray-500">Connecting to Drasi server...</p>
        </div>
      </div>
    );
  }

  // Show error state if initialization failed
  if (clientError) {
    return (
      <div className="min-h-screen bg-gray-50 flex items-center justify-center">
        <div className="text-center max-w-md">
          <AlertCircle className="w-12 h-12 text-red-500 mx-auto mb-4" />
          <h2 className="text-xl font-semibold text-gray-900 mb-2">Connection Error</h2>
          <p className="text-sm text-gray-600 mb-4">{clientError}</p>
          <p className="text-xs text-gray-500">
            Please ensure the Drasi server is running on port 8080
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-gray-50 flex flex-col">
      {/* Header */}
      <header className="bg-white border-b border-gray-200 shadow-sm">
        <div className="px-6 py-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 bg-gradient-to-br from-blue-500 to-purple-600 rounded-lg flex items-center justify-center shadow-md">
                <Database className="w-6 h-6 text-white" />
              </div>
              <div>
                <h1 className="text-2xl font-semibold text-gray-900">Drasi Playground</h1>
                <p className="text-sm text-gray-500">Interactive continuous query experimentation</p>
              </div>
            </div>
            <div className="flex items-center gap-4">
              {/* Connection Status */}
              <div className="flex items-center gap-2 px-3 py-1.5 bg-gray-100 rounded-lg">
                {getConnectionStatusIcon()}
                <span className="text-sm font-medium text-gray-700">
                  {getConnectionStatusText()}
                </span>
              </div>
              <button className="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-lg hover:bg-gray-50 transition-all duration-150 ease-out">
                Documentation
              </button>
            </div>
          </div>
        </div>
      </header>

      {/* Main Content */}
      <div className="flex-1 flex overflow-hidden">
        {/* Left Panel */}
        <div className="w-1/2 bg-white border-r border-gray-200 flex flex-col">
          {/* Tab Navigation */}
          <div className="flex border-b border-gray-200 bg-gray-50">
            <button
              onClick={() => setActiveTab('sources')}
              className={`flex-1 px-4 py-3 text-sm font-medium transition-all duration-150 ease-out ${
                activeTab === 'sources'
                  ? 'text-blue-600 border-b-2 border-blue-600 bg-blue-50/50'
                  : 'text-gray-500 hover:text-gray-700 hover:bg-gray-100'
              }`}
            >
              <div className="flex items-center justify-center gap-2">
                <Database className="w-4 h-4" />
                <span>Sources</span>
              </div>
            </button>
            <button
              onClick={() => setActiveTab('data')}
              className={`flex-1 px-4 py-3 text-sm font-medium transition-all duration-150 ease-out ${
                activeTab === 'data'
                  ? 'text-blue-600 border-b-2 border-blue-600 bg-blue-50/50'
                  : 'text-gray-500 hover:text-gray-700 hover:bg-gray-100'
              }`}
              disabled={!selectedSourceId}
            >
              <div className="flex items-center justify-center gap-2">
                <Activity className="w-4 h-4" />
                <span>Data</span>
              </div>
            </button>
            <button
              onClick={() => setActiveTab('queries')}
              className={`flex-1 px-4 py-3 text-sm font-medium transition-all duration-150 ease-out ${
                activeTab === 'queries'
                  ? 'text-blue-600 border-b-2 border-blue-600 bg-blue-50/50'
                  : 'text-gray-500 hover:text-gray-700 hover:bg-gray-100'
              }`}
            >
              <div className="flex items-center justify-center gap-2">
                <Code className="w-4 h-4" />
                <span>Queries</span>
              </div>
            </button>
          </div>

          {/* Content Area */}
          <div className="flex-1 overflow-hidden flex flex-col">
            {activeTab === 'sources' && (
              <div className="flex-1 overflow-auto">
                <SourceManager
                  onSourceSelect={(sourceId) => {
                    setSelectedSourceId(sourceId);
                    setActiveTab('data');
                  }}
                  selectedSourceId={selectedSourceId}
                />
              </div>
            )}

            {activeTab === 'data' && selectedSourceId && (
              <div className="flex-1 overflow-hidden flex flex-col">
                <div className="px-4 py-3 bg-gray-50 border-b border-gray-200">
                  <div className="flex items-center justify-between">
                    <h3 className="text-sm font-semibold text-gray-700">
                      Source: {selectedSourceId}
                    </h3>
                    <span className="text-xs text-gray-500">
                      Inject data into this source
                    </span>
                  </div>
                </div>
                <div className="flex-1 overflow-auto">
                  <DataTable
                    sourceId={selectedSourceId}
                    client={client!}
                  />
                </div>
              </div>
            )}

            {activeTab === 'queries' && (
              <div className="flex-1 overflow-hidden">
                <QueryManager
                  defaultSourceId={selectedSourceId}
                  onQuerySelect={(queryId) => setSelectedQueryId(queryId)}
                  selectedQueryId={selectedQueryId}
                />
              </div>
            )}
          </div>
        </div>

        {/* Right Panel - Query Results */}
        <div className="w-1/2 bg-gray-50 flex flex-col">
          {selectedQueryId ? (
            <QueryResults queryId={selectedQueryId} />
          ) : (
            <div className="flex-1 flex items-center justify-center">
              <div className="text-center">
                <Code className="w-12 h-12 text-gray-300 mx-auto mb-4" />
                <h3 className="text-lg font-semibold text-gray-600 mb-2">
                  No Query Selected
                </h3>
                <p className="text-sm text-gray-500 max-w-xs">
                  Create or select a query to see results here
                </p>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default App;