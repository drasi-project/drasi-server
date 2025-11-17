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

import { useState, useMemo, useEffect } from 'react';
import {
  useReactTable,
  getCoreRowModel,
  getSortedRowModel,
  getFilteredRowModel,
  getPaginationRowModel,
  flexRender,
  ColumnDef,
  SortingState,
  ColumnFiltersState,
} from '@tanstack/react-table';
import { useForm } from 'react-hook-form';
import { DataTableProps, DataEvent } from '@/types';
import { useSourceData } from '@/contexts/SourceDataContext';

// Example data for demo - matches the query templates
const EXAMPLE_DATA = [
  { id: 'prod-001', name: 'Gaming Laptop', category: 'Electronics', price: 1299.99, stock: 5 },
  { id: 'prod-002', name: 'Wireless Mouse', category: 'Electronics', price: 29.99, stock: 50 },
  { id: 'prod-003', name: 'Mechanical Keyboard', category: 'Electronics', price: 149.99, stock: 25 },
  { id: 'prod-004', name: '4K Monitor', category: 'Electronics', price: 399.99, stock: 8 },
  { id: 'prod-005', name: 'USB-C Hub', category: 'Electronics', price: 49.99, stock: 3 },
  { id: 'prod-006', name: 'Ergonomic Chair', category: 'Furniture', price: 299.99, stock: 12 },
  { id: 'prod-007', name: 'Standing Desk', category: 'Furniture', price: 599.99, stock: 7 },
  { id: 'prod-008', name: 'Desk Lamp', category: 'Furniture', price: 39.99, stock: 20 },
  { id: 'prod-009', name: 'Notebook Set', category: 'Stationery', price: 19.99, stock: 100 },
  { id: 'prod-010', name: 'Pen Pack', category: 'Stationery', price: 9.99, stock: 200 },
];

export function DataTable({ sourceId, typeLabel, sourceName, client }: DataTableProps) {
  // Use context for persistent data storage across tab switches
  const { getSourceTypeData, setSourceTypeData, getOriginalData, setOriginalData } = useSourceData();

  // Initialize local state from context
  const [data, setData] = useState<any[]>([]);
  const [originalData, setOriginalDataLocal] = useState<Map<string, any>>(new Map());
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showAddForm, setShowAddForm] = useState(false);
  const [editingRow, setEditingRow] = useState<string | null>(null);
  const [editData, setEditData] = useState<Record<string, any>>({});
  const [sorting, setSorting] = useState<SortingState>([]);
  const [columnFilters, setColumnFilters] = useState<ColumnFiltersState>([]);
  const [globalFilter, setGlobalFilter] = useState('');

  // Load data from context when component mounts or sourceId/typeLabel changes
  useEffect(() => {
    const contextData = getSourceTypeData(sourceId, typeLabel);
    const contextOriginalData = getOriginalData(sourceId, typeLabel);
    console.log(`Loading data for source ${sourceId}/${typeLabel}:`, contextData.length, 'items');
    setData(contextData);
    setOriginalDataLocal(contextOriginalData);
  }, [sourceId, typeLabel, getSourceTypeData, getOriginalData]);

  // Helper to update both local and context data
  const updateData = (newData: any[]) => {
    console.log(`Saving data for source ${sourceId}/${typeLabel}:`, newData.length, 'items');
    setData(newData);
    setSourceTypeData(sourceId, typeLabel, newData);
  };

  const updateOriginalData = (newOriginalData: Map<string, any>) => {
    setOriginalDataLocal(newOriginalData);
    setOriginalData(sourceId, typeLabel, newOriginalData);
  };

  const { register, handleSubmit, reset, formState: { errors } } = useForm<Record<string, any>>({
    defaultValues: {},
  });

  // Extract columns from data
  const columns = useMemo<ColumnDef<any>[]>(() => {
    if (!data || data.length === 0) return [];

    const keys = Object.keys(data[0]);
    const cols: ColumnDef<any>[] = keys.map((key) => ({
      accessorKey: key,
      header: key,
      cell: ({ row, getValue }) => {
        const isEditing = editingRow === row.original.id;
        const value = getValue() as any;

        if (isEditing) {
          return (
            <input
              type="text"
              value={editData[key] ?? value}
              onChange={(e) => setEditData({ ...editData, [key]: e.target.value })}
              className="w-full px-2 py-1 bg-white border border-gray-300 rounded text-sm focus:ring-2 focus:ring-blue-500 focus:border-blue-500 focus:outline-none text-gray-900"
            />
          );
        }

        return <span className="text-sm text-gray-900">{String(value)}</span>;
      },
    }));

    // Add actions column
    cols.push({
      id: 'actions',
      header: 'Actions',
      cell: ({ row }) => {
        const isEditing = editingRow === row.original.id;

        if (isEditing) {
          return (
            <div className="flex gap-2">
              <button
                onClick={() => handleSaveEdit(row.original)}
                className="px-2 py-1 bg-green-600 text-white rounded text-xs hover:bg-green-700 transition-colors"
              >
                Save
              </button>
              <button
                onClick={() => {
                  setEditingRow(null);
                  setEditData({});
                }}
                className="px-2 py-1 bg-gray-200 text-gray-700 rounded text-xs hover:bg-gray-300 transition-colors"
              >
                Cancel
              </button>
            </div>
          );
        }

        return (
          <div className="flex gap-2">
            <button
              onClick={() => handleStartEdit(row.original)}
              className="px-2 py-1 bg-blue-600 text-white rounded text-xs hover:bg-blue-700 transition-colors"
            >
              Edit
            </button>
            <button
              onClick={() => handleDelete(row.original)}
              className="px-2 py-1 bg-red-600 text-white rounded text-xs hover:bg-red-700 transition-colors"
            >
              Delete
            </button>
          </div>
        );
      },
    });

    return cols;
  }, [data, editingRow, editData]);

  const table = useReactTable({
    data: data || [],
    columns,
    state: {
      sorting,
      columnFilters,
      globalFilter,
    },
    onSortingChange: setSorting,
    onColumnFiltersChange: setColumnFilters,
    onGlobalFilterChange: setGlobalFilter,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    initialState: {
      pagination: {
        pageSize: 10,
      },
    },
  });

  const handleStartEdit = (row: any) => {
    setEditingRow(row.id);
    setEditData({ ...row });
  };

  const handleSaveEdit = async (originalRow: any) => {
    if (!client) return;

    // Get the before data from our tracked original data
    const beforeData = originalData.get(originalRow.id) || originalRow;

    const event: DataEvent = {
      operation: 'update',
      element: {
        type: 'node',
        id: originalRow.id,
        labels: [typeLabel],
        properties: editData,
        before: beforeData,  // Include before state
        after: editData      // Include after state
      },
    };

    // Try to inject to server but don't fail if it doesn't work
    try {
      await client.injectData(sourceId, event);
    } catch (err: any) {
      console.warn(`Failed to inject update to source ${sourceId}:`, err.message);
    }

    // Always update local state
    const newData = data.map(row => row.id === originalRow.id ? { ...editData } : row);
    updateData(newData);

    const newOriginalData = new Map(originalData);
    newOriginalData.set(originalRow.id, { ...editData });
    updateOriginalData(newOriginalData);

    setEditingRow(null);
    setEditData({});
  };

  const handleDelete = async (row: any) => {
    if (!client) return;
    if (!confirm('Are you sure you want to delete this row?')) return;

    // Get the original data for the before state
    const beforeData = originalData.get(row.id) || row;

    const event: DataEvent = {
      operation: 'delete',
      element: {
        type: 'node',
        id: row.id,
        labels: [typeLabel],
        properties: {},       // Empty properties for delete
        before: beforeData    // Include the before state
      },
    };

    // Try to inject to server but don't fail if it doesn't work
    try {
      await client.injectData(sourceId, event);
    } catch (err: any) {
      console.warn(`Failed to inject delete to source ${sourceId}:`, err.message);
    }

    // Always update local state
    const newData = data.filter(r => r.id !== row.id);
    updateData(newData);

    const newOriginalData = new Map(originalData);
    newOriginalData.delete(row.id);
    updateOriginalData(newOriginalData);
  };

  const onSubmit = async (formData: Record<string, any>) => {
    if (!client) return;

    const id = formData.id || `prod-${Date.now()}`;
    const newRecord = { ...formData, id };

    const event: DataEvent = {
      operation: 'insert',
      element: {
        type: 'node',
        id,
        labels: [typeLabel],
        properties: newRecord,
        after: newRecord     // Include after state for insert
      },
    };

    // Try to inject to server but don't fail if it doesn't work
    try {
      await client.injectData(sourceId, event);
    } catch (err: any) {
      console.warn(`Failed to inject insert to source ${sourceId}:`, err.message);
    }

    // Always update local state
    const newData = [...data, newRecord];
    updateData(newData);

    const newOriginalData = new Map(originalData);
    newOriginalData.set(id, newRecord);
    updateOriginalData(newOriginalData);

    setShowAddForm(false);
    reset();
  };

  const loadExampleData = async () => {
    if (!client) return;

    setLoading(true);

    // Track all example data as original
    const newOriginalData = new Map<string, any>();
    let injectionFailed = false;

    // Try to inject data to server
    for (const item of EXAMPLE_DATA) {
      const event: DataEvent = {
        operation: 'insert',
        element: {
          type: 'node',
          id: item.id,
          labels: [typeLabel],
          properties: item,
          after: item  // Include after state for insert
        },
      };

      try {
        await client.injectData(sourceId, event);
      } catch (err: any) {
        // Log error but continue - we'll still store data locally
        console.warn(`Failed to inject data to source ${sourceId}:`, err.message);
        injectionFailed = true;
      }

      newOriginalData.set(item.id, item);
    }

    // Always update local state, even if injection failed
    updateData(EXAMPLE_DATA);
    updateOriginalData(newOriginalData);

    if (injectionFailed) {
      alert('Note: Data injection to server may have failed (this is normal for mock sources), but data has been stored locally for UI simulation.');
    } else {
      alert(`Example data loaded successfully! The data has been injected with "${typeLabel}" labels.`);
    }

    setLoading(false);
  };

  const exportToCSV = () => {
    if (!data || data.length === 0) return;

    const headers = Object.keys(data[0]);
    const csvContent = [
      headers.join(','),
      ...data.map(row => headers.map(header => JSON.stringify(row[header] || '')).join(','))
    ].join('\n');

    const blob = new Blob([csvContent], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${sourceName}-data.csv`;
    a.click();
    URL.revokeObjectURL(url);
  };

  // Get field names from existing data
  const fieldNames = useMemo(() => {
    if (!data || data.length === 0) return ['id', 'name', 'value'];
    return Object.keys(data[0]).filter((key) => key !== 'id');
  }, [data]);

  if (loading) {
    return (
      <div className="space-y-4 animate-pulse">
        <div className="h-8 bg-gray-200 rounded-xl w-1/4"></div>
        <div className="h-48 bg-gray-200 rounded-xl"></div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="bg-red-50 border border-red-200 rounded-xl p-4 shadow-sm">
        <div className="text-red-700 font-semibold">Error Loading Data</div>
        <div className="text-red-600 text-sm mt-1">{error}</div>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {/* Compact Header with Actions */}
      <div className="flex items-center justify-between mb-3">
        <div className="text-xs text-gray-400">
          {data.length} {data.length === 1 ? 'row' : 'rows'}
        </div>

        <div className="flex gap-2">
          {data.length === 0 && (
            <button
              onClick={loadExampleData}
              className="px-2.5 py-1 bg-blue-600 text-white hover:bg-blue-700 rounded text-xs transition-colors"
              disabled={loading}
            >
              Load Example Data
            </button>
          )}
          {data.length > 0 && (
            <button
              onClick={exportToCSV}
              className="px-2.5 py-1 bg-gray-100 text-gray-700 hover:bg-gray-200 rounded text-xs transition-colors"
            >
              Export CSV
            </button>
          )}
          <button
            onClick={() => setShowAddForm(true)}
            className="px-2.5 py-1 bg-green-600 text-white hover:bg-green-700 rounded text-xs transition-colors"
          >
            + Add Row
          </button>
        </div>
      </div>

      {/* Search Bar */}
      {data.length > 0 && (
        <div className="flex items-center gap-2 bg-white border border-gray-200 rounded-lg px-3 py-2">
          <svg className="w-4 h-4 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
          </svg>
          <input
            type="text"
            value={globalFilter ?? ''}
            onChange={(e) => setGlobalFilter(e.target.value)}
            placeholder="Search all columns..."
            className="flex-1 bg-transparent border-0 focus:ring-0 focus:outline-none text-gray-900 text-sm placeholder-gray-400"
          />
          {globalFilter && (
            <button
              onClick={() => setGlobalFilter('')}
              className="text-xs text-gray-400 hover:text-gray-200"
            >
              Clear
            </button>
          )}
        </div>
      )}

      {/* Data Table */}
      <div className="bg-white border border-gray-200 rounded-lg overflow-hidden shadow-sm">
        {data.length === 0 ? (
          <div className="p-8 text-center">
            <div className="text-gray-600 mb-3">
              <svg className="w-10 h-10 mx-auto" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4" />
              </svg>
            </div>
            <p className="text-sm text-gray-400 mb-4">No data yet</p>
            <button
              onClick={() => setShowAddForm(true)}
              className="px-4 py-2 bg-green-600 text-white text-xs rounded hover:bg-green-700 transition-colors"
            >
              Add First Row
            </button>
          </div>
        ) : (
          <>
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead className="bg-gray-50 border-b border-gray-200">
                  {table.getHeaderGroups().map((headerGroup) => (
                    <tr key={headerGroup.id}>
                      {headerGroup.headers.map((header) => (
                        <th
                          key={header.id}
                          className="px-4 py-3 text-left text-xs font-semibold text-gray-700 uppercase tracking-wider cursor-pointer hover:text-gray-900 transition-colors"
                          onClick={header.column.getToggleSortingHandler()}
                        >
                          <div className="flex items-center gap-2">
                            {flexRender(header.column.columnDef.header, header.getContext())}
                            {header.column.getIsSorted() && (
                              <span className="text-blue-400">
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
                      className="hover:bg-blue-50/50 transition-colors"
                    >
                      {row.getVisibleCells().map((cell) => (
                        <td key={cell.id} className="px-4 py-2 text-gray-700">
                          {flexRender(cell.column.columnDef.cell, cell.getContext())}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>

            {/* Pagination */}
            {table.getPageCount() > 1 && (
              <div className="border-t border-gray-200 bg-gray-50 px-4 py-3">
                <div className="flex flex-col sm:flex-row gap-3 items-center justify-between">
                  <div className="text-xs text-gray-400">
                    Showing {table.getState().pagination.pageIndex * table.getState().pagination.pageSize + 1} to{' '}
                    {Math.min(
                      (table.getState().pagination.pageIndex + 1) * table.getState().pagination.pageSize,
                      data.length
                    )}{' '}
                    of {data.length} rows
                  </div>

                  <div className="flex items-center gap-1">
                    <button
                      onClick={() => table.setPageIndex(0)}
                      disabled={!table.getCanPreviousPage()}
                      className="px-2 py-1 bg-gray-100 border border-gray-300 text-gray-700 rounded hover:bg-gray-200 disabled:opacity-50 disabled:cursor-not-allowed transition-colors text-xs"
                    >
                      First
                    </button>
                    <button
                      onClick={() => table.previousPage()}
                      disabled={!table.getCanPreviousPage()}
                      className="px-2 py-1 bg-gray-100 border border-gray-300 text-gray-700 rounded hover:bg-gray-200 disabled:opacity-50 disabled:cursor-not-allowed transition-colors text-xs"
                    >
                      Previous
                    </button>
                    <span className="px-3 py-1 text-xs text-gray-700">
                      Page {table.getState().pagination.pageIndex + 1} of {table.getPageCount()}
                    </span>
                    <button
                      onClick={() => table.nextPage()}
                      disabled={!table.getCanNextPage()}
                      className="px-2 py-1 bg-gray-100 border border-gray-300 text-gray-700 rounded hover:bg-gray-200 disabled:opacity-50 disabled:cursor-not-allowed transition-colors text-xs"
                    >
                      Next
                    </button>
                    <button
                      onClick={() => table.setPageIndex(table.getPageCount() - 1)}
                      disabled={!table.getCanNextPage()}
                      className="px-2 py-1 bg-gray-100 border border-gray-300 text-gray-700 rounded hover:bg-gray-200 disabled:opacity-50 disabled:cursor-not-allowed transition-colors text-xs"
                    >
                      Last
                    </button>
                  </div>
                </div>
              </div>
            )}
          </>
        )}
      </div>

      {/* Add Row Modal */}
      {showAddForm && (
        <div className="fixed inset-0 bg-black/50 backdrop-blur-sm flex items-center justify-center z-50 p-4">
          <div className="bg-white border border-gray-200 rounded-lg p-6 max-w-md w-full shadow-xl">
            <div className="flex items-center justify-between mb-6">
              <h3 className="text-lg font-semibold text-gray-900">
                Add New Row
              </h3>
              <button
                onClick={() => {
                  setShowAddForm(false);
                  reset();
                }}
                className="text-gray-400 hover:text-gray-200"
              >
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>

            <form onSubmit={handleSubmit(onSubmit)} className="space-y-3">
              <div>
                <label className="block text-xs font-medium mb-1.5 text-gray-400">ID (Optional)</label>
                <input
                  {...register('id')}
                  type="text"
                  className="w-full px-3 py-2 bg-white border border-gray-300 rounded focus:ring-2 focus:ring-blue-500 focus:border-blue-500 focus:outline-none transition-colors text-gray-900 text-sm placeholder-gray-500"
                  placeholder="Auto-generated if empty"
                />
              </div>

              {fieldNames.map((field) => (
                <div key={field}>
                  <label className="block text-xs font-medium mb-1.5 text-gray-400">{field}</label>
                  <input
                    {...register(field)}
                    type="text"
                    className="w-full px-3 py-2 bg-white border border-gray-300 rounded focus:ring-2 focus:ring-blue-500 focus:border-blue-500 focus:outline-none transition-colors text-gray-900 text-sm placeholder-gray-500"
                    placeholder={`Enter ${field}`}
                  />
                </div>
              ))}

              <div className="flex gap-2 justify-end pt-4 border-t border-gray-200">
                <button
                  type="button"
                  onClick={() => {
                    setShowAddForm(false);
                    reset();
                  }}
                  className="px-4 py-2 bg-gray-100 text-gray-700 rounded hover:bg-gray-200 transition-colors text-sm"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="px-4 py-2 bg-green-600 text-white rounded hover:bg-green-700 transition-colors text-sm font-medium"
                >
                  Add Row
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}
