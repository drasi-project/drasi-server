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
import { useSources } from '@/hooks/useDrasi';
import { Source } from '@/types';

interface SourceManagerProps {
  onSourceSelect?: (sourceId: string) => void;
  selectedSourceId?: string | null;
}

export function SourceManager({ onSourceSelect, selectedSourceId }: SourceManagerProps) {
  const { sources, loading, error, createSource, deleteSource } = useSources();
  const [hasCreatedDemoSource, setHasCreatedDemoSource] = useState(false);

  // Automatically create demo-source if it doesn't exist
  useEffect(() => {
    if (!loading && sources.length === 0 && !hasCreatedDemoSource && createSource) {
      setHasCreatedDemoSource(true);
      const createDemoSource = async () => {
        try {
          await createSource({
            kind: 'http',
            id: 'demo-source',
            autoStart: true,
            host: '0.0.0.0',
            port: 9000,
            timeoutMs: 10000,
          } as any);
          console.log('Created demo-source automatically');
        } catch (err) {
          console.error('Failed to auto-create demo-source:', err);
        }
      };
      createDemoSource();
    }
  }, [loading, sources, hasCreatedDemoSource, createSource]);

  const [showCreateForm, setShowCreateForm] = useState(false);
  const [formData, setFormData] = useState({
    id: '',
    auto_start: true,
  });
  const [creating, setCreating] = useState(false);
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    setCreating(true);

    try {
      // Always create HTTP sources for the playground
      const sourceConfig: any = {
        kind: 'http',
        id: formData.id,
        autoStart: formData.auto_start,
        host: '0.0.0.0',
        port: 9000 + sources.length,
        timeoutMs: 10000,
      };

      await createSource(sourceConfig);
      setShowCreateForm(false);
      setFormData({ id: '', auto_start: true });
    } catch (err: any) {
      alert(`Failed to create source: ${err.message}`);
    } finally {
      setCreating(false);
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await deleteSource(id);
      setDeleteConfirm(null);
    } catch (err: any) {
      alert(`Failed to delete source: ${err.message}`);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center py-12">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500"></div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="bg-red-50 border border-red-200 rounded-xl p-4">
        <p className="text-sm font-medium text-red-800">Error: {error}</p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-slate-900">Data Sources</h2>
          <p className="text-sm text-slate-600 mt-1">Create simulated data sources to test continuous queries</p>
        </div>
        <div className="flex gap-2">
          {sources.length === 0 && (
            <button
              onClick={async () => {
                setCreating(true);
                try {
                  // Always create HTTP sources
                  await createSource({
                    id: 'product-catalog',
                    source_type: 'http',
                    auto_start: true,
                    host: '0.0.0.0',
                    port: 9000 + sources.length,
                    timeout_ms: 10000,
                    dispatch_buffer_capacity: 1000,
                  });
                  alert('Example source "product-catalog" created! Now go to the Data tab to load example products.');
                } catch (err: any) {
                  alert(`Failed to create example source: ${err.message}`);
                } finally {
                  setCreating(false);
                }
              }}
              className="inline-flex items-center gap-2 px-4 py-2.5 bg-green-500 text-white text-sm font-medium rounded-lg hover:bg-green-600 transition-colors shadow-sm"
              disabled={creating}
            >
              <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
              </svg>
              Create Example Source
            </button>
          )}
          <button
            onClick={() => setShowCreateForm(true)}
            className="inline-flex items-center gap-2 px-4 py-2.5 bg-blue-500 text-white text-sm font-medium rounded-lg hover:bg-blue-600 transition-colors shadow-sm"
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
            </svg>
            Create Source
          </button>
        </div>
      </div>

      {/* Sources Table */}
      <div className="bg-white border border-gray-200 rounded-xl shadow-sm overflow-hidden">
        <table className="w-full">
          <thead>
            <tr className="bg-gray-50 border-b border-gray-200">
              <th className="px-6 py-3 text-left text-xs font-semibold text-gray-700 uppercase tracking-wider">
                Source ID
              </th>
              <th className="px-6 py-3 text-left text-xs font-semibold text-gray-700 uppercase tracking-wider">
                Status
              </th>
              <th className="px-6 py-3 text-left text-xs font-semibold text-gray-700 uppercase tracking-wider">
                Auto Start
              </th>
              <th className="px-6 py-3 text-right text-xs font-semibold text-gray-700 uppercase tracking-wider">
                Actions
              </th>
            </tr>
          </thead>
          <tbody className="divide-y divide-gray-200">
            {sources.length === 0 ? (
              <tr>
                <td colSpan={4} className="px-6 py-12 text-center">
                  <svg className="w-12 h-12 text-gray-400 mx-auto mb-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M4 7v10c0 2 1 3 3 3h10c2 0 3-1 3-3V7c0-2-1-3-3-3H7c-2 0-3 1-3 3z" />
                  </svg>
                  <p className="text-sm font-medium text-gray-900 mb-1">No sources yet</p>
                  <p className="text-sm text-gray-600">Create your first data source to get started</p>
                </td>
              </tr>
            ) : (
              sources.map((source) => (
                <tr
                  key={source.id}
                  className={`hover:bg-gray-50 transition-colors cursor-pointer ${
                    selectedSourceId === source.id ? 'bg-blue-50' : ''
                  }`}
                  onClick={() => onSourceSelect?.(source.id)}
                >
                  <td className="px-6 py-4">
                    <span className="text-sm font-semibold text-slate-900 font-mono">{source.id}</span>
                  </td>
                  <td className="px-6 py-4">
                    <span
                      className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs font-medium ${
                        source.status === 'Running'
                          ? 'bg-green-50 text-green-700'
                          : 'bg-gray-100 text-gray-600'
                      }`}
                    >
                      <span
                        className={`w-1.5 h-1.5 rounded-full ${
                          source.status === 'Running' ? 'bg-green-500' : 'bg-gray-400'
                        }`}
                      ></span>
                      {source.status || 'Unknown'}
                    </span>
                  </td>
                  <td className="px-6 py-4">
                    <span className="text-sm text-slate-600">{source.auto_start ? 'Yes' : 'No'}</span>
                  </td>
                  <td className="px-6 py-4 text-right">
                    {deleteConfirm === source.id ? (
                      <div className="inline-flex items-center gap-2">
                        <button
                          onClick={() => handleDelete(source.id)}
                          className="px-3 py-1.5 bg-red-500 text-white text-xs font-medium rounded-md hover:bg-red-600 transition-colors"
                        >
                          Confirm Delete
                        </button>
                        <button
                          onClick={() => setDeleteConfirm(null)}
                          className="px-3 py-1.5 bg-gray-200 text-gray-700 text-xs font-medium rounded-md hover:bg-gray-300 transition-colors"
                        >
                          Cancel
                        </button>
                      </div>
                    ) : (
                      <button
                        onClick={() => setDeleteConfirm(source.id)}
                        className="px-3 py-1.5 bg-red-50 text-red-600 text-xs font-medium rounded-md hover:bg-red-100 transition-colors"
                      >
                        Delete
                      </button>
                    )}
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>


      {/* Create Form Modal */}
      {showCreateForm && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
          <div className="bg-white rounded-xl shadow-2xl max-w-md w-full p-6">
            <div className="flex items-center justify-between mb-6">
              <h3 className="text-xl font-bold text-slate-900">Create New Source</h3>
              <button
                onClick={() => setShowCreateForm(false)}
                className="text-gray-400 hover:text-gray-600 transition-colors"
              >
                <svg className="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>

            <form onSubmit={handleCreate} className="space-y-4">
              <div>
                <label className="block text-sm font-medium text-slate-700 mb-1.5">Source ID</label>
                <input
                  type="text"
                  value={formData.id}
                  onChange={(e) => setFormData({ ...formData, id: e.target.value })}
                  className="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
                  placeholder="my-data-source"
                  required
                />
              </div>

              <div className="flex items-center gap-2 p-3 bg-gray-50 rounded-lg">
                <input
                  type="checkbox"
                  id="auto_start"
                  checked={formData.auto_start}
                  onChange={(e) => setFormData({ ...formData, auto_start: e.target.checked })}
                  className="w-4 h-4 text-blue-500 border-gray-300 rounded focus:ring-blue-500"
                />
                <label htmlFor="auto_start" className="text-sm text-slate-700">
                  Auto-start source
                </label>
              </div>

              <div className="flex items-center gap-3 pt-4">
                <button
                  type="button"
                  onClick={() => setShowCreateForm(false)}
                  className="flex-1 px-4 py-2.5 bg-gray-100 text-gray-700 text-sm font-medium rounded-lg hover:bg-gray-200 transition-colors"
                  disabled={creating}
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="flex-1 px-4 py-2.5 bg-blue-500 text-white text-sm font-medium rounded-lg hover:bg-blue-600 transition-colors disabled:opacity-50"
                  disabled={creating}
                >
                  {creating ? 'Creating...' : 'Create Source'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}
