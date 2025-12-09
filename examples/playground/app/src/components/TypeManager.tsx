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
import { Plus, Database, X } from 'lucide-react';
import { DataTable } from './DataTable';
import { useSourceData } from '@/contexts/SourceDataContext';

interface TypeManagerProps {
  sourceId: string;
  sourceName?: string;
  client: any;
}

export function TypeManager({ sourceId, sourceName, client }: TypeManagerProps) {
  const { getSourceTypes, setSourceTypeData, clearSourceTypeData } = useSourceData();
  const [selectedType, setSelectedType] = useState<string | null>(null);
  const [showAddType, setShowAddType] = useState(false);
  const [newTypeName, setNewTypeName] = useState('');

  // Get types from context - this is the source of truth
  const types = getSourceTypes(sourceId);

  // Auto-select first type when types change
  useEffect(() => {
    if (types.length > 0 && !selectedType) {
      setSelectedType(types[0]);
    } else if (types.length === 0) {
      setSelectedType(null);
    }
  }, [types, selectedType]);

  const handleAddType = () => {
    if (newTypeName && !types.includes(newTypeName)) {
      // Initialize empty data for the new type in context
      setSourceTypeData(sourceId, newTypeName, []);
      setSelectedType(newTypeName);
      setNewTypeName('');
      setShowAddType(false);
    }
  };

  const handleRemoveType = (type: string) => {
    if (confirm(`Remove type "${type}" and all its data?`)) {
      clearSourceTypeData(sourceId, type);
      if (selectedType === type) {
        const remainingTypes = types.filter(t => t !== type);
        setSelectedType(remainingTypes.length > 0 ? remainingTypes[0] : null);
      }
    }
  };

  // Predefined common types for quick add
  const commonTypes = ['Product', 'Customer', 'Order', 'User', 'Transaction', 'Event'];
  const suggestedTypes = commonTypes.filter(t => !types.includes(t));

  return (
    <div className="flex flex-col h-full">
      {/* Type Tabs */}
      <div className="bg-gray-50 border-b border-gray-200">
        <div className="px-4 py-2">
          <div className="flex items-center justify-between mb-2">
            <h3 className="text-sm font-semibold text-gray-700">
              Data Types in {sourceName || sourceId}
            </h3>
            <button
              onClick={() => setShowAddType(true)}
              className="text-xs px-2 py-1 bg-blue-500 text-white rounded hover:bg-blue-600 transition-colors flex items-center gap-1"
            >
              <Plus className="w-3 h-3" />
              Add Type
            </button>
          </div>

          {/* Type Selection Tabs */}
          <div className="flex gap-1 flex-wrap">
            {types.map(type => (
              <button
                key={type}
                onClick={() => setSelectedType(type)}
                className={`px-3 py-1.5 text-xs font-medium rounded-md transition-all ${
                  selectedType === type
                    ? 'bg-white text-blue-600 shadow-sm border border-gray-200'
                    : 'bg-gray-100 text-gray-600 hover:bg-gray-200'
                }`}
              >
                <div className="flex items-center gap-1.5">
                  <Database className="w-3 h-3" />
                  {type}
                  {types.length > 1 && (
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        handleRemoveType(type);
                      }}
                      className="ml-1 hover:text-red-500"
                    >
                      <X className="w-3 h-3" />
                    </button>
                  )}
                </div>
              </button>
            ))}

            {types.length === 0 && (
              <div className="text-xs text-gray-500 py-2">
                No types yet. Add a type to start managing data.
              </div>
            )}
          </div>

          {/* Quick Add Suggestions */}
          {!showAddType && suggestedTypes.length > 0 && types.length === 0 && (
            <div className="mt-2 pt-2 border-t border-gray-200">
              <p className="text-xs text-gray-500 mb-1">Quick add:</p>
              <div className="flex gap-1 flex-wrap">
                {suggestedTypes.slice(0, 4).map(type => (
                  <button
                    key={type}
                    onClick={() => {
                      // Initialize empty data for the new type in context
                      setSourceTypeData(sourceId, type, []);
                      setSelectedType(type);
                    }}
                    className="px-2 py-1 text-xs bg-blue-50 text-blue-600 rounded hover:bg-blue-100 transition-colors"
                  >
                    + {type}
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>

        {/* Add Type Form */}
        {showAddType && (
          <div className="px-4 py-2 border-t border-gray-200 bg-blue-50">
            <div className="flex items-center gap-2">
              <input
                type="text"
                placeholder="Enter type name (e.g., Product, Customer)"
                value={newTypeName}
                onChange={(e) => setNewTypeName(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && handleAddType()}
                className="flex-1 px-2 py-1 text-sm border border-gray-300 rounded focus:outline-none focus:ring-1 focus:ring-blue-500"
                autoFocus
              />
              <button
                onClick={handleAddType}
                disabled={!newTypeName}
                className="px-3 py-1 text-xs bg-blue-500 text-white rounded hover:bg-blue-600 disabled:bg-gray-300 transition-colors"
              >
                Add
              </button>
              <button
                onClick={() => {
                  setShowAddType(false);
                  setNewTypeName('');
                }}
                className="px-3 py-1 text-xs bg-gray-500 text-white rounded hover:bg-gray-600 transition-colors"
              >
                Cancel
              </button>
            </div>
          </div>
        )}
      </div>

      {/* Data Table for Selected Type */}
      {selectedType ? (
        <div className="flex-1 overflow-hidden">
          <DataTable
            sourceId={sourceId}
            typeLabel={selectedType}
            sourceName={sourceName}
            client={client}
          />
        </div>
      ) : (
        <div className="flex-1 flex items-center justify-center">
          <div className="text-center">
            <Database className="w-12 h-12 text-gray-300 mx-auto mb-3" />
            <p className="text-gray-500 text-sm">
              Add a data type to start managing records
            </p>
          </div>
        </div>
      )}
    </div>
  );
}