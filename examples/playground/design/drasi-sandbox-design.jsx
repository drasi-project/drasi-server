import React, { useState } from 'react';
import { Play, Plus, Trash2, Edit3, Database, Code2, FileJson, RefreshCw } from 'lucide-react';

const DrasiSandboxDesign = () => {
  const [activeTab, setActiveTab] = useState('source');
  const [sourceData, setSourceData] = useState([
    { id: 1, name: 'Alice', age: 28, city: 'New York', department: 'Engineering' },
    { id: 2, name: 'Bob', age: 35, city: 'San Francisco', department: 'Marketing' },
    { id: 3, name: 'Charlie', age: 42, city: 'London', department: 'Sales' },
  ]);

  return (
    <div className="min-h-screen bg-gray-50">
      {/* Header */}
      <header className="bg-white border-b border-gray-200">
        <div className="px-6 py-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center space-x-3">
              <div className="w-10 h-10 bg-gradient-to-br from-blue-500 to-purple-600 rounded-lg flex items-center justify-center">
                <Database className="w-6 h-6 text-white" />
              </div>
              <div>
                <h1 className="text-2xl font-semibold text-gray-900">Drasi Playground</h1>
                <p className="text-sm text-gray-500">Interactive continuous query experimentation</p>
              </div>
            </div>
            <div className="flex items-center space-x-4">
              <button className="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-lg hover:bg-gray-50 transition-colors">
                Documentation
              </button>
              <button className="px-4 py-2 text-sm font-medium text-white bg-gradient-to-r from-blue-500 to-purple-600 rounded-lg hover:from-blue-600 hover:to-purple-700 transition-all shadow-sm">
                Share Sandbox
              </button>
            </div>
          </div>
        </div>
      </header>

      {/* Main Layout */}
      <div className="flex h-[calc(100vh-88px)]">
        {/* Left Panel - Source Data & Query Editor */}
        <div className="w-1/2 bg-white border-r border-gray-200">
          {/* Tabs */}
          <div className="flex border-b border-gray-200">
            <button
              onClick={() => setActiveTab('source')}
              className={`flex-1 px-4 py-3 text-sm font-medium transition-all ${
                activeTab === 'source'
                  ? 'text-blue-600 border-b-2 border-blue-600 bg-blue-50/50'
                  : 'text-gray-600 hover:text-gray-900 hover:bg-gray-50'
              }`}
            >
              <div className="flex items-center justify-center space-x-2">
                <Database className="w-4 h-4" />
                <span>Source Data</span>
              </div>
            </button>
            <button
              onClick={() => setActiveTab('query')}
              className={`flex-1 px-4 py-3 text-sm font-medium transition-all ${
                activeTab === 'query'
                  ? 'text-blue-600 border-b-2 border-blue-600 bg-blue-50/50'
                  : 'text-gray-600 hover:text-gray-900 hover:bg-gray-50'
              }`}
            >
              <div className="flex items-center justify-center space-x-2">
                <Code2 className="w-4 h-4" />
                <span>Query Editor</span>
              </div>
            </button>
          </div>

          {/* Tab Content */}
          <div className="h-full overflow-hidden">
            {activeTab === 'source' ? (
              <div className="h-full flex flex-col">
                {/* Source Data Toolbar */}
                <div className="px-4 py-3 border-b border-gray-200 bg-gray-50">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center space-x-2">
                      <button className="px-3 py-1.5 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 transition-colors flex items-center space-x-1">
                        <Plus className="w-4 h-4" />
                        <span>Add Row</span>
                      </button>
                      <button className="px-3 py-1.5 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50 transition-colors">
                        Import CSV
                      </button>
                    </div>
                    <div className="text-sm text-gray-500">
                      {sourceData.length} rows
                    </div>
                  </div>
                </div>

                {/* Data Table */}
                <div className="flex-1 overflow-auto">
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
                        <tr key={row.id} className="hover:bg-gray-50 transition-colors">
                          <td className="px-4 py-3 text-sm text-gray-900">{row.id}</td>
                          <td className="px-4 py-3 text-sm text-gray-900">{row.name}</td>
                          <td className="px-4 py-3 text-sm text-gray-900">{row.age}</td>
                          <td className="px-4 py-3 text-sm text-gray-900">{row.city}</td>
                          <td className="px-4 py-3 text-sm text-gray-900">{row.department}</td>
                          <td className="px-4 py-3 text-sm text-gray-900">
                            <div className="flex items-center space-x-2">
                              <button className="text-gray-400 hover:text-blue-600 transition-colors">
                                <Edit3 className="w-4 h-4" />
                              </button>
                              <button className="text-gray-400 hover:text-red-600 transition-colors">
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
            ) : (
              <div className="h-full flex flex-col">
                {/* Query Editor Toolbar */}
                <div className="px-4 py-3 border-b border-gray-200 bg-gray-50">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center space-x-2">
                      <select className="px-3 py-1.5 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50 transition-colors">
                        <option>Cypher</option>
                        <option>GQL</option>
                      </select>
                      <button className="px-3 py-1.5 text-sm font-medium text-white bg-green-600 rounded-md hover:bg-green-700 transition-colors flex items-center space-x-1">
                        <Play className="w-4 h-4" />
                        <span>Execute Query</span>
                      </button>
                    </div>
                    <div className="flex items-center space-x-2">
                      <button className="px-3 py-1.5 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50 transition-colors">
                        Format
                      </button>
                      <button className="px-3 py-1.5 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50 transition-colors">
                        Examples
                      </button>
                    </div>
                  </div>
                </div>

                {/* Code Editor */}
                <div className="flex-1 p-4">
                  <div className="h-full bg-gray-900 rounded-lg p-4 font-mono text-sm">
                    <pre className="text-gray-300">
{`MATCH (p:Person)
WHERE p.age > 30
RETURN p.name, p.age, p.department
ORDER BY p.age DESC`}
                    </pre>
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Right Panel - Results */}
        <div className="w-1/2 bg-white">
          <div className="h-full flex flex-col">
            {/* Results Header */}
            <div className="px-4 py-3 border-b border-gray-200 bg-gray-50">
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-3">
                  <h2 className="text-lg font-medium text-gray-900">Query Results</h2>
                  <div className="flex items-center space-x-2 text-sm text-gray-500">
                    <RefreshCw className="w-4 h-4 text-green-500 animate-spin" />
                    <span>Live Updates Enabled</span>
                  </div>
                </div>
                <div className="flex items-center space-x-2">
                  <button className="px-3 py-1.5 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50 transition-colors">
                    Export
                  </button>
                </div>
              </div>
            </div>

            {/* Results Display */}
            <div className="flex-1 overflow-auto p-4">
              <div className="bg-gray-50 rounded-lg p-4 mb-4">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-sm font-medium text-gray-700">Query Status</span>
                  <span className="px-2 py-1 text-xs font-medium text-green-800 bg-green-100 rounded-full">Active</span>
                </div>
                <div className="text-sm text-gray-500">
                  Last updated: 2 seconds ago • 2 rows returned
                </div>
              </div>

              {/* Results Table */}
              <div className="bg-white border border-gray-200 rounded-lg overflow-hidden">
                <table className="w-full">
                  <thead className="bg-gray-50">
                    <tr>
                      <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Name</th>
                      <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Age</th>
                      <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Department</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-gray-200">
                    <tr className="bg-yellow-50 transition-all">
                      <td className="px-4 py-3 text-sm text-gray-900">Charlie</td>
                      <td className="px-4 py-3 text-sm text-gray-900">42</td>
                      <td className="px-4 py-3 text-sm text-gray-900">Sales</td>
                    </tr>
                    <tr>
                      <td className="px-4 py-3 text-sm text-gray-900">Bob</td>
                      <td className="px-4 py-3 text-sm text-gray-900">35</td>
                      <td className="px-4 py-3 text-sm text-gray-900">Marketing</td>
                    </tr>
                  </tbody>
                </table>
              </div>

              {/* Change Log */}
              <div className="mt-6">
                <h3 className="text-sm font-medium text-gray-700 mb-3">Recent Changes</h3>
                <div className="space-y-2">
                  <div className="flex items-start space-x-2 text-sm">
                    <div className="w-1.5 h-1.5 bg-blue-500 rounded-full mt-1.5"></div>
                    <div className="flex-1">
                      <span className="text-gray-600">Row updated: Charlie's age changed from 41 to 42</span>
                      <span className="text-gray-400 text-xs ml-2">5s ago</span>
                    </div>
                  </div>
                  <div className="flex items-start space-x-2 text-sm">
                    <div className="w-1.5 h-1.5 bg-green-500 rounded-full mt-1.5"></div>
                    <div className="flex-1">
                      <span className="text-gray-600">New row added: ID #4</span>
                      <span className="text-gray-400 text-xs ml-2">12s ago</span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

export default DrasiSandboxDesign;