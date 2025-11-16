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

import { useState } from 'react';
import {
  Database,
  Code,
  Play,
  Plus,
  Upload,
  Edit2,
  Trash2,
  Download,
  CheckCircle2,
  Clock
} from 'lucide-react';

function App() {
  const [activeTab, setActiveTab] = useState<'source' | 'query'>('source');
  const [liveUpdates, setLiveUpdates] = useState(true);

  // Mock data for demonstration
  const sourceData = [
    { id: 1, name: 'Alice', age: 28, city: 'New York', department: 'Engineering' },
    { id: 2, name: 'Bob', age: 35, city: 'San Francisco', department: 'Marketing' },
    { id: 3, name: 'Charlie', age: 42, city: 'London', department: 'Sales' },
  ];

  const queryResults = [
    { name: 'Charlie', age: 42, department: 'Sales' },
    { name: 'Bob', age: 35, department: 'Marketing' },
  ];

  const recentChanges = [
    { type: 'update', message: "Charlie's age changed from 41 to 42", time: '5s ago' },
    { type: 'add', message: 'New row added: ID #4', time: '12s ago' },
  ];

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
            <div className="flex items-center gap-3">
              <button className="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-lg hover:bg-gray-50 transition-all duration-150 ease-out">
                Documentation
              </button>
              <button className="px-4 py-2 text-sm font-medium text-white bg-gradient-to-r from-blue-500 to-purple-600 rounded-lg hover:from-blue-600 hover:to-purple-700 transition-all duration-150 shadow-sm hover:shadow-md">
                Share Sandbox
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
              onClick={() => setActiveTab('source')}
              className={`flex-1 px-4 py-3 text-sm font-medium transition-all duration-150 ease-out ${
                activeTab === 'source'
                  ? 'text-blue-600 border-b-2 border-blue-600 bg-blue-50/50'
                  : 'text-gray-500 hover:text-gray-700 hover:bg-gray-100'
              }`}
            >
              <div className="flex items-center justify-center gap-2">
                <Database className="w-4 h-4" />
                <span>Source Data</span>
              </div>
            </button>
            <button
              onClick={() => setActiveTab('query')}
              className={`flex-1 px-4 py-3 text-sm font-medium transition-all duration-150 ease-out ${
                activeTab === 'query'
                  ? 'text-blue-600 border-b-2 border-blue-600 bg-blue-50/50'
                  : 'text-gray-500 hover:text-gray-700 hover:bg-gray-100'
              }`}
            >
              <div className="flex items-center justify-center gap-2">
                <Code className="w-4 h-4" />
                <span>Query Editor</span>
              </div>
            </button>
          </div>

          {/* Content Area */}
          <div className="flex-1 overflow-auto">
            {activeTab === 'source' ? (
              <div className="h-full flex flex-col">
                {/* Source Data Toolbar */}
                <div className="px-4 py-3 border-b border-gray-200 bg-gray-50">
                  <div className="flex items-center justify-between">
                    <div className="flex gap-2">
                      <button className="flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium bg-blue-600 text-white rounded-md hover:bg-blue-700 transition-all duration-150 ease-out">
                        <Plus className="w-4 h-4" />
                        <span>Add Row</span>
                      </button>
                      <button className="flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium border border-gray-300 bg-white text-gray-700 rounded-md hover:bg-gray-50 transition-all duration-150 ease-out">
                        <Upload className="w-4 h-4" />
                        <span>Import CSV</span>
                      </button>
                    </div>
                    <div className="text-sm text-gray-500">
                      {sourceData.length} rows
                    </div>
                  </div>
                </div>

                {/* Data Table */}
                <div className="flex-1 overflow-auto p-4">
                  <div className="bg-white rounded-lg border border-gray-200 overflow-hidden">
                    <table className="w-full">
                      <thead className="bg-gray-50 sticky top-0">
                        <tr>
                          <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">ID</th>
                          <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Name</th>
                          <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Age</th>
                          <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">City</th>
                          <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Department</th>
                          <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Actions</th>
                        </tr>
                      </thead>
                      <tbody className="bg-white divide-y divide-gray-200">
                        {sourceData.map((row) => (
                          <tr key={row.id} className="hover:bg-gray-50 transition-colors duration-150 ease-out">
                            <td className="px-4 py-3 text-sm text-gray-900">{row.id}</td>
                            <td className="px-4 py-3 text-sm text-gray-900">{row.name}</td>
                            <td className="px-4 py-3 text-sm text-gray-900">{row.age}</td>
                            <td className="px-4 py-3 text-sm text-gray-900">{row.city}</td>
                            <td className="px-4 py-3 text-sm text-gray-900">{row.department}</td>
                            <td className="px-4 py-3">
                              <div className="flex gap-1">
                                <button className="p-1 text-gray-400 hover:text-blue-600 transition-colors duration-150 ease-out">
                                  <Edit2 className="w-4 h-4" />
                                </button>
                                <button className="p-1 text-gray-400 hover:text-red-600 transition-colors duration-150 ease-out">
                                  <Trash2 className="w-4 h-4" />
                                </button>
                              </div>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </div>
              </div>
            ) : (
              <div className="flex flex-col h-full">
                {/* Query Editor Toolbar */}
                <div className="px-4 py-3 border-b border-gray-200 bg-gray-50">
                  <div className="flex items-center justify-between">
                    <div className="flex gap-2">
                      <select className="px-3 py-1.5 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:border-gray-400 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500 transition-all duration-150">
                        <option>Cypher</option>
                        <option>GQL</option>
                      </select>
                      <button className="flex items-center gap-1.5 px-3 py-1.5 bg-green-600 text-white text-sm font-medium rounded-md hover:bg-green-700 transition-all duration-150 ease-out">
                        <Play className="w-4 h-4" />
                        <span>Execute Query</span>
                      </button>
                    </div>
                    <div className="flex gap-2">
                      <button className="px-3 py-1.5 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50 transition-all duration-150">
                        Format
                      </button>
                      <button className="px-3 py-1.5 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50 transition-all duration-150">
                        Examples
                      </button>
                    </div>
                  </div>
                </div>

                {/* Query Editor */}
                <div className="flex-1 p-4">
                  <div className="h-full bg-gray-900 rounded-lg p-4 font-mono text-sm shadow-inner">
                  <pre className="text-gray-300 leading-relaxed">
<span className="text-purple-400">MATCH</span> <span className="text-yellow-300">(p:Person)</span>
<span className="text-purple-400">WHERE</span> p.department <span className="text-purple-400">IN</span> <span className="text-orange-400">['Sales', 'Marketing']</span>
<span className="text-purple-400">RETURN</span> p.name, p.age, p.department
<span className="text-purple-400">ORDER BY</span> p.age <span className="text-purple-400">DESC</span>
                    </pre>
                  </div>
                </div>

                {/* Query Info */}
                <div className="px-4 py-3 border-t border-gray-200 bg-gray-50">
                  <div className="flex items-center gap-4 text-sm text-gray-500">
                  <div className="flex items-center gap-1.5">
                    <CheckCircle2 className="w-4 h-4 text-green-500" />
                    <span>Valid Cypher syntax</span>
                  </div>
                  <div className="flex items-center gap-1.5">
                    <Clock className="w-4 h-4 text-gray-400" />
                    <span>Last run: 2 seconds ago</span>
                  </div>
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Right Panel */}
        <div className="w-1/2 bg-gray-50 flex flex-col">
          {/* Results Header */}
          <div className="bg-white border-b border-gray-200 px-6 py-3">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <h2 className="text-lg font-semibold text-gray-900">Query Results</h2>
                <div className="flex items-center gap-1.5 px-2 py-1 bg-green-100 text-green-700 rounded-full text-xs font-medium">
                  <div className="w-2 h-2 bg-green-500 rounded-full animate-pulse" />
                  <span>Live Updates</span>
                </div>
              </div>
              <button className="flex items-center gap-2 px-3 py-1.5 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-lg hover:bg-gray-50 transition-all duration-150">
                <Download className="w-4 h-4" />
                <span>Export</span>
              </button>
            </div>
          </div>

          {/* Query Status */}
          <div className="bg-white px-6 py-4 border-b border-gray-200">
            <div className="bg-gray-50 rounded-lg p-3">
              <div className="flex items-center justify-between">
                <div>
                  <div className="text-sm font-semibold text-gray-700">Query Status</div>
                  <div className="text-xs text-gray-500 mt-1">
                    Last updated: <span className="font-medium text-gray-600">2 seconds ago</span> • <span className="font-medium text-gray-600">{queryResults.length} rows</span> returned
                  </div>
                </div>
                <div className="px-2.5 py-1 bg-green-100 text-green-800 rounded-full text-xs font-semibold">
                  Active
                </div>
              </div>
            </div>
          </div>

          {/* Results Content */}
          <div className="flex-1 overflow-auto p-6 bg-gray-50">
            {/* Results Table */}
            <div className="bg-white border border-gray-200 rounded-lg overflow-hidden shadow-sm mb-6">
              <table className="w-full">
                <thead className="bg-gray-50">
                  <tr className="border-b border-gray-200">
                    <th className="px-4 py-3 text-left text-xs font-semibold text-gray-700 uppercase tracking-wider">Name</th>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-gray-700 uppercase tracking-wider">Age</th>
                    <th className="px-4 py-3 text-left text-xs font-semibold text-gray-700 uppercase tracking-wider">Department</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-gray-100">
                  {queryResults.map((row, idx) => (
                    <tr key={idx} className={`${idx === 0 ? 'bg-yellow-50 border-l-4 border-yellow-400' : 'hover:bg-gray-50'} transition-all duration-150 ease-out`}>
                      <td className="px-4 py-3 text-sm text-gray-900 font-medium">{row.name}</td>
                      <td className="px-4 py-3 text-sm text-gray-900">{row.age}</td>
                      <td className="px-4 py-3 text-sm text-gray-900">{row.department}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>

            {/* Recent Changes */}
            <div className="bg-white rounded-lg p-4 border border-gray-200 shadow-sm">
              <h3 className="text-sm font-semibold text-gray-900 mb-3">Recent Changes</h3>
              <div className="space-y-2">
                {recentChanges.map((change, idx) => (
                  <div key={idx} className="flex items-start gap-2.5 text-sm p-2 hover:bg-gray-50 rounded transition-colors duration-150">
                    <div className={`w-2 h-2 rounded-full mt-1.5 ${
                      change.type === 'update' ? 'bg-blue-500 animate-pulse' : 'bg-green-500'
                    }`} />
                    <div className="flex-1">
                      <span className="text-gray-700">{change.message}</span>
                      <span className="text-gray-400 text-xs ml-2 font-medium">{change.time}</span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

export default App;