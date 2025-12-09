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

import { useState, useMemo } from 'react';
import Editor from '@monaco-editor/react';
import {
  useReactTable,
  getCoreRowModel,
  getSortedRowModel,
  flexRender,
  ColumnDef,
  SortingState,
} from '@tanstack/react-table';
import { useForm, Controller } from 'react-hook-form';
import { useQueries, useSources } from '@/hooks/useDrasi';
import { Query } from '@/types';

interface QueryFormData {
  id: string;
  query: string;
  sources: string[];
  auto_start: boolean;
}

const QUERY_TEMPLATES = [
  {
    name: 'All Products',
    query: 'MATCH (p:Product)\nRETURN elementId(p) AS id, p.name AS name, p.category AS category, p.price AS price, p.stock AS stock',
    description: 'Returns all products from the source',
  },
  {
    name: 'Low Stock Items',
    query: 'MATCH (p:Product)\nWHERE p.stock < 10\nRETURN elementId(p) AS id, p.name AS name, p.stock AS stock, p.category AS category',
    description: 'Find products with low inventory (stock < 10)',
  },
  {
    name: 'Electronics Category',
    query: "MATCH (p:Product)\nWHERE p.category = 'Electronics'\nRETURN elementId(p) AS id, p.name AS name, p.price AS price, p.stock AS stock",
    description: 'Filter products in Electronics category',
  },
  {
    name: 'Price Range $100-$500',
    query: 'MATCH (p:Product)\nWHERE p.price >= 100 AND p.price <= 500\nRETURN elementId(p) AS id, p.name AS name, p.price AS price\nORDER BY p.price DESC',
    description: 'Find products priced between $100 and $500',
  },
  {
    name: 'Category Summary',
    query: 'MATCH (p:Product)\nRETURN p.category AS category, COUNT(p) AS product_count, AVG(p.price) AS avg_price\nORDER BY product_count DESC',
    description: 'Aggregate products by category with count and average price',
  },
];

interface QueryManagerProps {
  defaultSourceId?: string | null;
  onQuerySelect?: (queryId: string) => void;
  selectedQueryId?: string | null;
}

export function QueryManager({ defaultSourceId, onQuerySelect, selectedQueryId }: QueryManagerProps) {
  const { queries, loading, error, createQuery, deleteQuery } = useQueries();
  const { sources } = useSources();
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [creating, setCreating] = useState(false);
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);
  const [sorting, setSorting] = useState<SortingState>([]);

  const { register, handleSubmit, reset, control, setValue, formState: { errors }, watch } = useForm<QueryFormData>({
    defaultValues: {
      id: '',
      query: 'MATCH (n) RETURN n',
      sources: defaultSourceId ? [defaultSourceId] : [],
      auto_start: true,
    },
  });

  const selectedSources = watch('sources');

  const columns = useMemo<ColumnDef<Query>[]>(
    () => [
      {
        accessorKey: 'id',
        header: 'Query ID',
        cell: ({ getValue }) => (
          <span className="font-mono text-sm font-semibold text-slate-900">{getValue() as string}</span>
        ),
      },
      {
        accessorKey: 'query',
        header: 'Cypher Query',
        cell: ({ getValue }) => {
          const query = getValue() as string;
          return (
            <div className="max-w-md">
              <code className="text-sm text-slate-700 block truncate font-mono bg-gray-50 px-3 py-1.5 rounded-lg border border-gray-200" title={query}>
                {query}
              </code>
            </div>
          );
        },
      },
      {
        accessorKey: 'sources',
        header: 'Sources',
        cell: ({ getValue }) => {
          const sources = getValue() as string[] | undefined;
          if (!sources || sources.length === 0) {
            return <span className="text-gray-400 text-sm">No sources</span>;
          }
          return (
            <div className="flex flex-wrap gap-2">
              {sources.map((source) => (
                <span
                  key={source}
                  className="px-2.5 py-1 bg-purple-50 text-purple-700 rounded-lg text-xs font-medium border border-purple-200"
                >
                  {source}
                </span>
              ))}
            </div>
          );
        },
      },
      {
        accessorKey: 'status',
        header: 'Status',
        cell: ({ getValue }) => {
          const status = getValue() as string;
          return (
            <span
              className={`inline-flex items-center gap-2 px-3 py-1 rounded-lg font-medium text-sm ${
                status === 'Running'
                  ? 'bg-green-50 text-green-700 border border-green-200'
                  : 'bg-gray-50 text-gray-600 border border-gray-200'
              }`}
            >
              <span className={`w-2 h-2 rounded-full ${status === 'Running' ? 'bg-green-500' : 'bg-gray-400'}`}></span>
              {status || 'Unknown'}
            </span>
          );
        },
      },
      {
        id: 'actions',
        header: 'Actions',
        cell: ({ row }) => (
          <div className="flex gap-2 justify-end">
            {deleteConfirm === row.original.id ? (
              <div className="inline-flex gap-2">
                <button
                  onClick={() => handleDelete(row.original.id)}
                  className="px-3 py-1.5 bg-red-600 text-white rounded-lg text-sm hover:bg-red-700 transition-all font-medium"
                >
                  Confirm
                </button>
                <button
                  onClick={() => setDeleteConfirm(null)}
                  className="px-3 py-1.5 bg-gray-200 text-gray-700 rounded-lg text-sm hover:bg-gray-300 transition-all font-medium"
                >
                  Cancel
                </button>
              </div>
            ) : (
              <button
                onClick={() => setDeleteConfirm(row.original.id)}
                className="px-3 py-1.5 bg-red-50 text-red-700 rounded-lg text-sm hover:bg-red-100 transition-all border border-red-200 font-medium"
              >
                Delete
              </button>
            )}
          </div>
        ),
      },
    ],
    [deleteConfirm]
  );

  const table = useReactTable({
    data: queries,
    columns,
    state: {
      sorting,
    },
    onSortingChange: setSorting,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
  });

  const onSubmit = async (data: QueryFormData) => {
    setCreating(true);

    try {
      const queryConfig: Partial<Query> = {
        id: data.id,
        query: data.query,
        sources: data.sources,
        auto_start: data.auto_start,
      };

      const newQuery = await createQuery(queryConfig);
      setShowCreateForm(false);
      reset();
      // Auto-select the newly created query
      if (onQuerySelect && newQuery) {
        onQuerySelect(data.id);
      }
    } catch (err: any) {
      alert(`Failed to create query: ${err.message}`);
    } finally {
      setCreating(false);
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await deleteQuery(id);
      setDeleteConfirm(null);
    } catch (err: any) {
      alert(`Failed to delete query: ${err.message}`);
    }
  };

  const toggleSource = (sourceId: string) => {
    const current = selectedSources || [];
    if (current.includes(sourceId)) {
      setValue('sources', current.filter((s) => s !== sourceId));
    } else {
      setValue('sources', [...current, sourceId]);
    }
  };

  const applyTemplate = (templateQuery: string) => {
    setValue('query', templateQuery);
  };

  if (loading) {
    return (
      <div className="space-y-6 animate-pulse">
        <div className="h-12 bg-gray-200 rounded-xl w-1/3"></div>
        <div className="h-64 bg-gray-200 rounded-xl"></div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="bg-red-50 border border-red-200 rounded-xl p-6 shadow-sm">
        <div className="text-red-700 font-semibold text-lg">Error Loading Queries</div>
        <div className="text-red-600 mt-2">{error}</div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header with Action Button */}
      <div className="flex justify-between items-center">
        <div>
          <h1 className="text-3xl font-bold text-slate-900">Continuous Queries</h1>
          <p className="text-slate-600 mt-1">Define Cypher queries for real-time data processing</p>
        </div>
        <button
          onClick={() => setShowCreateForm(true)}
          className="px-6 py-2.5 bg-purple-600 text-white rounded-lg hover:bg-purple-700 transition-all shadow-sm hover:shadow font-semibold flex items-center gap-2"
        >
          <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
          </svg>
          Create Query
        </button>
      </div>

      {/* Queries Table */}
      <div className="bg-white border border-gray-200 rounded-xl overflow-hidden shadow-sm">
        {queries.length === 0 ? (
          <div className="p-16 text-center">
            <svg className="w-16 h-16 text-gray-400 mx-auto mb-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
            </svg>
            <h3 className="text-xl font-semibold text-slate-900 mb-2">No Queries Yet</h3>
            <p className="text-slate-600 mb-6">Create your first continuous query to start processing data</p>
            <button
              onClick={() => setShowCreateForm(true)}
              className="px-6 py-2.5 bg-purple-600 text-white rounded-lg hover:bg-purple-700 transition-all shadow-sm font-medium"
            >
              Create Your First Query
            </button>
          </div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-gray-50 border-b border-gray-200">
                {table.getHeaderGroups().map((headerGroup) => (
                  <tr key={headerGroup.id}>
                    {headerGroup.headers.map((header) => (
                      <th
                        key={header.id}
                        className="px-6 py-3 text-left text-sm font-semibold text-slate-700 cursor-pointer hover:text-slate-900 transition-colors"
                        onClick={header.column.getToggleSortingHandler()}
                      >
                        <div className="flex items-center gap-2">
                          {flexRender(header.column.columnDef.header, header.getContext())}
                          {header.column.getIsSorted() && (
                            <span className="text-purple-600">
                              {header.column.getIsSorted() === 'asc' ? '↑' : '↓'}
                            </span>
                          )}
                        </div>
                      </th>
                    ))}
                  </tr>
                ))}
              </thead>
              <tbody className="divide-y divide-gray-200">
                {table.getRowModel().rows.map((row) => (
                  <tr
                    key={row.id}
                    className="hover:bg-gray-50 transition-colors"
                  >
                    {row.getVisibleCells().map((cell) => (
                      <td key={cell.id} className="px-6 py-4">
                        {flexRender(cell.column.columnDef.cell, cell.getContext())}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Create Form Modal */}
      {showCreateForm && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4 overflow-y-auto">
          <div className="bg-white rounded-2xl p-8 max-w-5xl w-full my-8 shadow-2xl">
            <div className="flex items-center gap-3 mb-6">
              <div className="w-12 h-12 bg-gradient-to-br from-purple-500 to-pink-600 rounded-xl flex items-center justify-center shadow-sm">
                <svg className="w-6 h-6 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                </svg>
              </div>
              <h3 className="text-2xl font-bold text-slate-900">
                Create Continuous Query
              </h3>
            </div>

            <form onSubmit={handleSubmit(onSubmit)} className="space-y-6">
              {/* Query ID */}
              <div>
                <label className="block text-sm font-semibold mb-2 text-slate-700">Query ID</label>
                <input
                  {...register('id', { required: 'Query ID is required' })}
                  type="text"
                  className="w-full px-4 py-2.5 bg-white border border-gray-300 rounded-lg focus:ring-2 focus:ring-purple-500 focus:border-purple-500 focus:outline-none transition-all text-slate-900 placeholder-gray-400"
                  placeholder="my-query"
                />
                {errors.id && <p className="text-red-600 text-sm mt-1">{errors.id.message}</p>}
              </div>

              {/* Query Templates */}
              <div>
                <label className="block text-sm font-semibold mb-3 text-slate-700">Query Templates (Click to Use)</label>
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
                  {QUERY_TEMPLATES.map((template, idx) => (
                    <button
                      key={idx}
                      type="button"
                      onClick={() => applyTemplate(template.query)}
                      className="p-4 bg-gray-50 border border-gray-200 rounded-xl hover:border-purple-300 hover:bg-purple-50 transition-all text-left group"
                    >
                      <div className="font-semibold text-sm text-slate-900 group-hover:text-purple-700 transition-colors mb-1">
                        {template.name}
                      </div>
                      <div className="text-xs text-slate-600">{template.description}</div>
                    </button>
                  ))}
                </div>
              </div>

              {/* Cypher Editor */}
              <div>
                <label className="block text-sm font-semibold mb-2 text-slate-700">Cypher Query</label>
                <div className="border border-gray-300 rounded-xl overflow-hidden shadow-sm">
                  <Controller
                    name="query"
                    control={control}
                    rules={{ required: 'Query is required' }}
                    render={({ field }) => (
                      <Editor
                        height="500px"
                        defaultLanguage="cypher"
                        theme="light"
                        value={field.value}
                        onChange={(value) => field.onChange(value || '')}
                        options={{
                          minimap: { enabled: true },
                          fontSize: 14,
                          lineNumbers: 'on',
                          scrollBeyondLastLine: false,
                          wordWrap: 'on',
                          padding: { top: 16, bottom: 16 },
                          roundedSelection: true,
                          fontFamily: "'Fira Code', 'Consolas', 'Monaco', monospace",
                          fontLigatures: true,
                        }}
                      />
                    )}
                  />
                </div>
                {errors.query && <p className="text-red-600 text-sm mt-2">{errors.query.message}</p>}
              </div>

              {/* Source Selection */}
              <div>
                <label className="block text-sm font-semibold mb-3 text-slate-700">
                  Select Data Sources ({selectedSources?.length || 0} selected)
                </label>
                {sources.length === 0 ? (
                  <div className="p-6 bg-yellow-50 border border-yellow-200 rounded-xl">
                    <p className="text-yellow-700 font-medium">No sources available</p>
                    <p className="text-sm text-yellow-600 mt-1">Create a source first before creating queries.</p>
                  </div>
                ) : (
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                    {sources.map((source) => (
                      <label
                        key={source.id}
                        className={`flex items-center gap-3 p-4 rounded-xl cursor-pointer transition-all border ${
                          selectedSources?.includes(source.id)
                            ? 'bg-purple-50 border-purple-300 shadow-sm'
                            : 'bg-gray-50 border-gray-200 hover:border-gray-300 hover:bg-gray-100'
                        }`}
                      >
                        <input
                          type="checkbox"
                          checked={selectedSources?.includes(source.id) || false}
                          onChange={() => toggleSource(source.id)}
                          className="w-5 h-5 rounded border-gray-300 text-purple-600 focus:ring-purple-500"
                        />
                        <div className="flex-1">
                          <div className="font-medium text-sm text-slate-900">{source.id}</div>
                          <div className="text-xs text-slate-600">{source.source_type}</div>
                        </div>
                      </label>
                    ))}
                  </div>
                )}
              </div>

              {/* Auto Start */}
              <div className="flex items-center gap-3 p-4 bg-gray-50 rounded-lg border border-gray-200">
                <input
                  {...register('auto_start')}
                  type="checkbox"
                  id="auto_start_query"
                  className="w-5 h-5 rounded border-gray-300 text-purple-600 focus:ring-purple-500"
                />
                <label htmlFor="auto_start_query" className="text-sm text-slate-700 font-medium cursor-pointer">
                  Auto-start query on creation
                </label>
              </div>

              {/* Form Actions */}
              <div className="flex gap-3 justify-end pt-6 border-t border-gray-200">
                <button
                  type="button"
                  onClick={() => {
                    setShowCreateForm(false);
                    reset();
                  }}
                  className="px-6 py-2.5 bg-gray-100 text-gray-700 rounded-lg hover:bg-gray-200 transition-all font-medium"
                  disabled={creating}
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="px-6 py-2.5 bg-purple-600 text-white rounded-lg hover:bg-purple-700 transition-all shadow-sm hover:shadow disabled:opacity-50 disabled:cursor-not-allowed font-semibold"
                  disabled={creating || !selectedSources?.length}
                >
                  {creating ? 'Creating Query...' : 'Create Query'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}
