import React, { useState } from 'react';
import { 
  Play, Plus, Settings, Database, Code2, 
  Activity, Clock, AlertCircle, CheckCircle,
  ChevronRight, ChevronDown, Layers, Table
} from 'lucide-react';

const DrasiIDEStyleDesign = () => {
  const [expandedSections, setExpandedSections] = useState({
    sources: true,
    queries: true,
    results: true
  });

  const toggleSection = (section) => {
    setExpandedSections(prev => ({
      ...prev,
      [section]: !prev[section]
    }));
  };

  return (
    <div className="h-screen bg-[#1e1e1e] text-gray-100 flex flex-col">
      {/* Top Bar */}
      <div className="bg-[#2d2d30] border-b border-[#3e3e42] px-4 py-2">
        <div className="flex items-center justify-between">
          <div className="flex items-center space-x-4">
            <div className="flex items-center space-x-2">
              <div className="w-8 h-8 bg-gradient-to-br from-cyan-400 to-blue-500 rounded flex items-center justify-center">
                <Database className="w-5 h-5 text-white" />
              </div>
              <span className="font-semibold">Drasi Playground</span>
            </div>
            <div className="flex items-center space-x-1 text-xs">
              <div className="px-2 py-1 bg-[#1e1e1e] rounded flex items-center space-x-1">
                <div className="w-2 h-2 bg-green-400 rounded-full"></div>
                <span>Connected</span>
              </div>
            </div>
          </div>
          <div className="flex items-center space-x-2">
            <button className="p-1.5 hover:bg-[#3e3e42] rounded transition-colors">
              <Settings className="w-4 h-4" />
            </button>
          </div>
        </div>
      </div>

      <div className="flex-1 flex overflow-hidden">
        {/* Left Sidebar - Explorer */}
        <div className="w-64 bg-[#252526] border-r border-[#3e3e42] flex flex-col">
          <div className="p-2 text-xs uppercase text-gray-400 font-semibold">Explorer</div>
          
          {/* Data Sources Section */}
          <div className="flex-1 overflow-y-auto">
            <div>
              <button 
                onClick={() => toggleSection('sources')}
                className="w-full px-2 py-1 flex items-center space-x-1 hover:bg-[#2d2d30] transition-colors"
              >
                {expandedSections.sources ? 
                  <ChevronDown className="w-3 h-3" /> : 
                  <ChevronRight className="w-3 h-3" />
                }
                <Database className="w-3 h-3 text-cyan-400" />
                <span className="text-sm">Data Sources</span>
              </button>
              {expandedSections.sources && (
                <div className="pl-6">
                  <div className="px-2 py-1 text-sm hover:bg-[#2d2d30] cursor-pointer flex items-center justify-between group">
                    <span>persons_table</span>
                    <span className="text-xs text-gray-500 group-hover:text-gray-300">5 rows</span>
                  </div>
                  <div className="px-2 py-1 text-sm hover:bg-[#2d2d30] cursor-pointer flex items-center justify-between group">
                    <span>departments</span>
                    <span className="text-xs text-gray-500 group-hover:text-gray-300">3 rows</span>
                  </div>
                  <button className="px-2 py-1 text-sm text-cyan-400 hover:bg-[#2d2d30] w-full text-left flex items-center space-x-1">
                    <Plus className="w-3 h-3" />
                    <span>Add Source</span>
                  </button>
                </div>
              )}
            </div>

            {/* Queries Section */}
            <div className="mt-4">
              <button 
                onClick={() => toggleSection('queries')}
                className="w-full px-2 py-1 flex items-center space-x-1 hover:bg-[#2d2d30] transition-colors"
              >
                {expandedSections.queries ? 
                  <ChevronDown className="w-3 h-3" /> : 
                  <ChevronRight className="w-3 h-3" />
                }
                <Code2 className="w-3 h-3 text-purple-400" />
                <span className="text-sm">Continuous Queries</span>
              </button>
              {expandedSections.queries && (
                <div className="pl-6">
                  <div className="px-2 py-1 text-sm hover:bg-[#2d2d30] cursor-pointer flex items-center space-x-2">
                    <div className="w-2 h-2 bg-green-400 rounded-full animate-pulse"></div>
                    <span>active_employees</span>
                  </div>
                  <div className="px-2 py-1 text-sm hover:bg-[#2d2d30] cursor-pointer flex items-center space-x-2">
                    <div className="w-2 h-2 bg-yellow-400 rounded-full"></div>
                    <span>department_stats</span>
                  </div>
                  <button className="px-2 py-1 text-sm text-purple-400 hover:bg-[#2d2d30] w-full text-left flex items-center space-x-1">
                    <Plus className="w-3 h-3" />
                    <span>New Query</span>
                  </button>
                </div>
              )}
            </div>
          </div>

          {/* Bottom Status */}
          <div className="p-3 border-t border-[#3e3e42]">
            <div className="text-xs text-gray-400">
              <div className="flex items-center justify-between mb-1">
                <span>Memory Usage</span>
                <span>2.4 MB</span>
              </div>
              <div className="w-full bg-[#1e1e1e] rounded-full h-1.5">
                <div className="bg-cyan-400 h-1.5 rounded-full" style={{width: '24%'}}></div>
              </div>
            </div>
          </div>
        </div>

        {/* Main Content Area */}
        <div className="flex-1 flex flex-col">
          {/* Editor Tabs */}
          <div className="bg-[#2d2d30] border-b border-[#3e3e42] flex">
            <div className="flex items-center bg-[#1e1e1e] border-r border-[#3e3e42]">
              <div className="px-3 py-1.5 flex items-center space-x-2 text-sm">
                <Code2 className="w-3 h-3 text-purple-400" />
                <span>active_employees.cypher</span>
                <button className="ml-2 hover:bg-[#3e3e42] rounded p-0.5">
                  <span className="text-xs">×</span>
                </button>
              </div>
            </div>
            <div className="flex items-center border-r border-[#3e3e42]">
              <div className="px-3 py-1.5 flex items-center space-x-2 text-sm text-gray-400">
                <Database className="w-3 h-3 text-cyan-400" />
                <span>persons_table</span>
              </div>
            </div>
          </div>

          {/* Split View - Editor and Results */}
          <div className="flex-1 flex">
            {/* Code Editor */}
            <div className="w-1/2 border-r border-[#3e3e42] flex flex-col">
              <div className="bg-[#2d2d30] px-3 py-2 flex items-center justify-between">
                <div className="flex items-center space-x-2">
                  <select className="bg-[#3e3e42] text-sm px-2 py-1 rounded border border-[#464647]">
                    <option>Cypher</option>
                    <option>GQL</option>
                  </select>
                  <span className="text-xs text-gray-400">Auto-save enabled</span>
                </div>
                <button className="px-3 py-1 bg-green-600 hover:bg-green-700 text-white text-sm rounded flex items-center space-x-1 transition-colors">
                  <Play className="w-3 h-3" />
                  <span>Run Query</span>
                </button>
              </div>
              
              {/* Monaco-style editor */}
              <div className="flex-1 bg-[#1e1e1e] p-4 font-mono text-sm">
                <div className="flex">
                  <div className="text-gray-500 text-right pr-4 select-none">
                    <div>1</div>
                    <div>2</div>
                    <div>3</div>
                    <div>4</div>
                    <div>5</div>
                  </div>
                  <div className="flex-1">
                    <div><span className="text-purple-400">MATCH</span> <span className="text-yellow-300">(p:Person)</span></div>
                    <div><span className="text-purple-400">WHERE</span> p.age <span className="text-cyan-400">&gt;</span> <span className="text-green-400">30</span></div>
                    <div><span className="text-purple-400">AND</span> p.department <span className="text-cyan-400">=</span> <span className="text-orange-400">'Engineering'</span></div>
                    <div><span className="text-purple-400">RETURN</span> p.name, p.age, p.city</div>
                    <div><span className="text-purple-400">ORDER BY</span> p.age <span className="text-purple-400">DESC</span></div>
                  </div>
                </div>
              </div>

              {/* Query Status Bar */}
              <div className="bg-[#252526] px-3 py-1 border-t border-[#3e3e42] flex items-center justify-between text-xs">
                <div className="flex items-center space-x-3">
                  <span className="text-gray-400">Ln 5, Col 21</span>
                  <span className="text-gray-400">UTF-8</span>
                </div>
                <div className="flex items-center space-x-2">
                  <CheckCircle className="w-3 h-3 text-green-400" />
                  <span className="text-green-400">Valid Syntax</span>
                </div>
              </div>
            </div>

            {/* Results Panel */}
            <div className="w-1/2 flex flex-col bg-[#1e1e1e]">
              {/* Results Header */}
              <div className="bg-[#2d2d30] px-3 py-2 border-b border-[#3e3e42]">
                <div className="flex items-center justify-between">
                  <div className="flex items-center space-x-3">
                    <h3 className="text-sm font-medium">Query Results</h3>
                    <div className="flex items-center space-x-1 text-xs text-green-400">
                      <Activity className="w-3 h-3" />
                      <span>Live</span>
                    </div>
                  </div>
                  <div className="flex items-center space-x-2 text-xs text-gray-400">
                    <Clock className="w-3 h-3" />
                    <span>Updated 2s ago</span>
                  </div>
                </div>
              </div>

              {/* Results Table */}
              <div className="flex-1 overflow-auto">
                <table className="w-full text-sm">
                  <thead className="bg-[#252526] sticky top-0">
                    <tr>
                      <th className="px-3 py-2 text-left font-medium text-gray-300">name</th>
                      <th className="px-3 py-2 text-left font-medium text-gray-300">age</th>
                      <th className="px-3 py-2 text-left font-medium text-gray-300">city</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr className="border-b border-[#3e3e42] hover:bg-[#252526]">
                      <td className="px-3 py-2">Alice Johnson</td>
                      <td className="px-3 py-2">35</td>
                      <td className="px-3 py-2">San Francisco</td>
                    </tr>
                    <tr className="border-b border-[#3e3e42] hover:bg-[#252526] bg-yellow-900/20">
                      <td className="px-3 py-2">Bob Smith</td>
                      <td className="px-3 py-2">42</td>
                      <td className="px-3 py-2">New York</td>
                    </tr>
                  </tbody>
                </table>
              </div>

              {/* Output Console */}
              <div className="h-32 border-t border-[#3e3e42] bg-[#1e1e1e] p-3 overflow-y-auto">
                <div className="text-xs font-mono space-y-1">
                  <div className="text-gray-400">[14:23:45] Query executed successfully</div>
                  <div className="text-green-400">[14:23:45] 2 rows returned in 12ms</div>
                  <div className="text-yellow-400">[14:23:48] Row updated: Bob Smith age changed</div>
                  <div className="text-cyan-400">[14:23:48] Query results automatically updated</div>
                </div>
              </div>
            </div>
          </div>
        </div>

        {/* Right Sidebar - Properties/Details */}
        <div className="w-64 bg-[#252526] border-l border-[#3e3e42] p-3">
          <h3 className="text-sm font-semibold mb-3">Query Properties</h3>
          
          <div className="space-y-3 text-sm">
            <div>
              <label className="text-xs text-gray-400 uppercase">Update Mode</label>
              <select className="w-full mt-1 bg-[#3e3e42] px-2 py-1 rounded border border-[#464647]">
                <option>Real-time</option>
                <option>Batch (5s)</option>
                <option>Manual</option>
              </select>
            </div>
            
            <div>
              <label className="text-xs text-gray-400 uppercase">Performance</label>
              <div className="mt-1 space-y-1 text-xs">
                <div className="flex justify-between">
                  <span className="text-gray-400">Execution Time</span>
                  <span>12ms</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-gray-400">Updates/min</span>
                  <span>24</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-gray-400">Total Updates</span>
                  <span>156</span>
                </div>
              </div>
            </div>

            <div>
              <label className="text-xs text-gray-400 uppercase">Schema</label>
              <div className="mt-1 bg-[#1e1e1e] rounded p-2 text-xs font-mono">
                <div className="text-cyan-400">Person</div>
                <div className="pl-2 text-gray-400">
                  <div>├─ name: string</div>
                  <div>├─ age: integer</div>
                  <div>├─ city: string</div>
                  <div>└─ department: string</div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Bottom Status Bar */}
      <div className="bg-[#007ACC] px-3 py-1 flex items-center justify-between text-xs">
        <div className="flex items-center space-x-4">
          <span>Drasi Playground v1.0.0</span>
          <span>•</span>
          <span>2 active queries</span>
          <span>•</span>
          <span>5.2k events processed</span>
        </div>
        <div className="flex items-center space-x-4">
          <span>Memory: 2.4MB / 10MB</span>
          <span>•</span>
          <span>Latency: 12ms</span>
        </div>
      </div>
    </div>
  );
};

export default DrasiIDEStyleDesign;