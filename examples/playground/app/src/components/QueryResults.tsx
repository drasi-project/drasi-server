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

import { useState, useEffect, useMemo } from 'react';
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
import { useQuery } from '@/hooks/useDrasi';
import { QueryResultsProps } from '@/types';

export function QueryResults({ queryId }: QueryResultsProps) {
  const { data, loading, error, lastUpdate, lastSSEEvent } = useQuery(queryId);
  const [flashKey, setFlashKey] = useState(0);
  const [sorting, setSorting] = useState<SortingState>([]);
  const [columnFilters, setColumnFilters] = useState<ColumnFiltersState>([]);
  const [globalFilter, setGlobalFilter] = useState('');
  const [flashingRows, setFlashingRows] = useState<Set<string>>(new Set());

  // Trigger flash animation on updates and track which rows changed
  useEffect(() => {
    if (lastUpdate && lastSSEEvent) {
      setFlashKey((prev) => prev + 1);

      // Identify which rows were affected by this SSE event
      const affectedRowIds = new Set<string>();

      if (lastSSEEvent.results) {
        for (const changeEvent of lastSSEEvent.results) {
          // Try to extract an ID from the changed data
          const rowData = changeEvent.data || changeEvent.after || changeEvent.before;
          if (rowData) {
            // Create a unique identifier for the row based on its content
            const rowId = JSON.stringify(rowData);
            affectedRowIds.add(rowId);
          }
        }
      }

      // Flash the affected rows
      setFlashingRows(affectedRowIds);

      // Clear the flash after animation completes
      const timer = setTimeout(() => {
        setFlashingRows(new Set());
      }, 1000);

      return () => clearTimeout(timer);
    }
  }, [lastUpdate, lastSSEEvent]);

  // Extract columns from data
  const columns = useMemo<ColumnDef<any>[]>(() => {
    if (!data || data.length === 0) return [];

    const keys = Object.keys(data[0]);
    return keys.map((key) => ({
      accessorKey: key,
      header: key,
      cell: ({ getValue }) => {
        const value = getValue();
        return (
          <div className="text-sm text-slate-700">
            {renderValue(value)}
          </div>
        );
      },
    }));
  }, [data]);

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

  // Format timestamp
  const formatTimestamp = (timestamp: number | null) => {
    if (!timestamp) return 'Never';
    const date = new Date(timestamp);
    return date.toLocaleString('en-US', {
      month: 'short',
      day: 'numeric',
      year: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });
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
    a.download = `query-${queryId}-results.csv`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const exportToJSON = () => {
    if (!data || data.length === 0) return;

    const jsonContent = JSON.stringify(data, null, 2);
    const blob = new Blob([jsonContent], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `query-${queryId}-results.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  if (!queryId) {
    return (
      <div className="bg-white border-2 border-dashed border-gray-300 rounded-xl p-16 text-center shadow-sm">
        <svg className="w-16 h-16 text-gray-400 mx-auto mb-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" />
        </svg>
        <h3 className="text-xl font-semibold text-slate-900 mb-2">Select a Query</h3>
        <p className="text-slate-600">Choose a query from the dropdown above to view its results</p>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="space-y-6 animate-pulse">
        <div className="bg-white border border-gray-200 rounded-xl p-6 shadow-sm">
          <div className="space-y-4">
            <div className="h-4 bg-gray-200 rounded-lg w-1/4"></div>
            <div className="h-4 bg-gray-200 rounded-lg w-1/2"></div>
            <div className="h-4 bg-gray-200 rounded-lg w-3/4"></div>
          </div>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="bg-red-50 border border-red-200 rounded-xl p-6 shadow-sm">
        <div className="text-red-700 font-semibold text-lg flex items-center gap-2">
          <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
          </svg>
          Error Loading Results
        </div>
        <div className="text-red-600 mt-2">{error}</div>
      </div>
    );
  }

  return (
    <div className="space-y-6" key={flashKey}>
      {/* Metadata Cards */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div className="bg-white border border-gray-200 rounded-xl p-6 shadow-sm">
          <div className="flex items-center gap-3">
            <div className="w-12 h-12 bg-purple-50 rounded-lg flex items-center justify-center">
              <svg className="w-6 h-6 text-purple-600" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
              </svg>
            </div>
            <div>
              <div className="text-xs text-slate-600 font-medium">Query ID</div>
              <div className="font-mono text-sm text-slate-900 font-semibold">{queryId}</div>
            </div>
          </div>
        </div>

        <div className="bg-white border border-gray-200 rounded-xl p-6 shadow-sm">
          <div className="flex items-center gap-3">
            <div className="w-12 h-12 bg-blue-50 rounded-lg flex items-center justify-center">
              <svg className="w-6 h-6 text-blue-600" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" />
              </svg>
            </div>
            <div>
              <div className="text-xs text-slate-600 font-medium">Row Count</div>
              <div className="font-mono text-sm text-slate-900 font-semibold">{data.length}</div>
            </div>
          </div>
        </div>

        <div className="bg-white border border-gray-200 rounded-xl p-6 shadow-sm">
          <div className="flex items-center gap-3">
            <div className="w-12 h-12 bg-green-50 rounded-lg flex items-center justify-center">
              <svg className="w-6 h-6 text-green-600" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
            </div>
            <div>
              <div className="text-xs text-slate-600 font-medium">Last Update</div>
              <div className="text-sm text-slate-900 font-semibold">{formatTimestamp(lastUpdate)}</div>
            </div>
          </div>
        </div>
      </div>

      {/* Search and Export Bar */}
      {data.length > 0 && (
        <div className="bg-white border border-gray-200 rounded-xl p-4 shadow-sm">
          <div className="flex flex-col md:flex-row gap-3 items-start md:items-center justify-between">
            <div className="flex-1 w-full md:w-auto flex items-center gap-3">
              <svg className="w-5 h-5 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
              </svg>
              <input
                type="text"
                value={globalFilter ?? ''}
                onChange={(e) => setGlobalFilter(e.target.value)}
                placeholder="Search across all columns..."
                className="flex-1 px-3 py-2 bg-white border-0 focus:ring-0 focus:outline-none text-slate-900 placeholder-gray-400"
              />
              {globalFilter && (
                <button
                  onClick={() => setGlobalFilter('')}
                  className="px-3 py-1.5 bg-gray-100 text-gray-700 rounded-lg hover:bg-gray-200 transition-all text-sm font-medium"
                >
                  Clear
                </button>
              )}
            </div>

            <div className="flex gap-2">
              <button
                onClick={exportToCSV}
                className="px-4 py-2 bg-blue-50 text-blue-700 rounded-lg hover:bg-blue-100 transition-all border border-blue-200 font-medium flex items-center gap-2 text-sm"
              >
                <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                </svg>
                CSV
              </button>
              <button
                onClick={exportToJSON}
                className="px-4 py-2 bg-purple-50 text-purple-700 rounded-lg hover:bg-purple-100 transition-all border border-purple-200 font-medium flex items-center gap-2 text-sm"
              >
                <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                </svg>
                JSON
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Results Table */}
      <div className="bg-white border border-gray-200 rounded-xl overflow-hidden shadow-sm">
        {data.length === 0 ? (
          <div className="p-16 text-center">
            <svg className="w-16 h-16 text-gray-400 mx-auto mb-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
            <h3 className="text-xl font-semibold text-slate-900 mb-2">No Results Yet</h3>
            <p className="text-slate-600">Query is running but has not produced any results.</p>
            <p className="text-slate-500 text-sm mt-2">Try adding some data to your sources.</p>
          </div>
        ) : (
          <>
            <div className="overflow-x-auto">
              <table className="w-full">
                <thead className="bg-gray-50 border-b border-gray-200">
                  {table.getHeaderGroups().map((headerGroup) => (
                    <tr key={headerGroup.id}>
                      {headerGroup.headers.map((header) => (
                        <th
                          key={header.id}
                          className="px-6 py-3 text-left font-semibold text-sm text-slate-700 cursor-pointer hover:text-slate-900 transition-colors"
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
                  {table.getRowModel().rows.map((row, idx) => {
                    const rowId = JSON.stringify(row.original);
                    const isFlashing = flashingRows.has(rowId);

                    return (
                      <tr
                        key={row.id}
                        className={`${
                          isFlashing
                            ? ''
                            : idx % 2 === 0
                              ? 'bg-white hover:bg-gray-50 transition-colors'
                              : 'bg-gray-50/50 hover:bg-gray-50 transition-colors'
                        }`}
                        style={isFlashing ? {
                          animation: 'flash 1s ease-in-out'
                        } : undefined}
                      >
                        {row.getVisibleCells().map((cell) => (
                          <td key={cell.id} className="px-6 py-4">
                            {flexRender(cell.column.columnDef.cell, cell.getContext())}
                          </td>
                        ))}
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>

            {/* Pagination */}
            {table.getPageCount() > 1 && (
              <div className="border-t border-gray-200 bg-gray-50 px-6 py-4">
                <div className="flex flex-col sm:flex-row gap-4 items-center justify-between">
                  <div className="text-sm text-slate-600">
                    Showing {table.getState().pagination.pageIndex * table.getState().pagination.pageSize + 1} to{' '}
                    {Math.min(
                      (table.getState().pagination.pageIndex + 1) * table.getState().pagination.pageSize,
                      data.length
                    )}{' '}
                    of {data.length} rows
                  </div>

                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => table.setPageIndex(0)}
                      disabled={!table.getCanPreviousPage()}
                      className="px-3 py-1.5 bg-white border border-gray-300 text-slate-700 rounded-lg hover:bg-gray-50 disabled:opacity-50 disabled:cursor-not-allowed transition-all text-sm font-medium"
                    >
                      First
                    </button>
                    <button
                      onClick={() => table.previousPage()}
                      disabled={!table.getCanPreviousPage()}
                      className="px-3 py-1.5 bg-white border border-gray-300 text-slate-700 rounded-lg hover:bg-gray-50 disabled:opacity-50 disabled:cursor-not-allowed transition-all text-sm font-medium"
                    >
                      Previous
                    </button>
                    <span className="px-4 py-1.5 text-sm text-slate-700 font-medium">
                      Page {table.getState().pagination.pageIndex + 1} of {table.getPageCount()}
                    </span>
                    <button
                      onClick={() => table.nextPage()}
                      disabled={!table.getCanNextPage()}
                      className="px-3 py-1.5 bg-white border border-gray-300 text-slate-700 rounded-lg hover:bg-gray-50 disabled:opacity-50 disabled:cursor-not-allowed transition-all text-sm font-medium"
                    >
                      Next
                    </button>
                    <button
                      onClick={() => table.setPageIndex(table.getPageCount() - 1)}
                      disabled={!table.getCanNextPage()}
                      className="px-3 py-1.5 bg-white border border-gray-300 text-slate-700 rounded-lg hover:bg-gray-50 disabled:opacity-50 disabled:cursor-not-allowed transition-all text-sm font-medium"
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

      {/* Real-time Update Indicator */}
      {lastUpdate && (
        <div className="bg-green-50 border border-green-200 rounded-xl p-4 shadow-sm">
          <div className="flex items-center gap-3">
            <div className="w-2 h-2 bg-green-500 rounded-full animate-pulse"></div>
            <span className="text-green-700 text-sm font-medium">
              Real-time updates active - Last updated {formatTimestamp(lastUpdate)}
            </span>
          </div>
        </div>
      )}

      {/* Latest SSE Event Panel */}
      {lastSSEEvent && (
        <div className="bg-white border border-gray-200 rounded-xl overflow-hidden shadow-sm">
          <div className="bg-gradient-to-r from-blue-50 to-purple-50 border-b border-gray-200 px-6 py-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <div className="w-10 h-10 bg-gradient-to-br from-blue-500 to-purple-600 rounded-lg flex items-center justify-center">
                  <svg className="w-5 h-5 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
                  </svg>
                </div>
                <div>
                  <h3 className="text-lg font-semibold text-slate-900">Latest SSE Event</h3>
                  <p className="text-sm text-slate-600">Raw data from Drasi Server</p>
                </div>
              </div>
              {lastSSEEvent.sequence !== undefined && (
                <div className="text-sm text-slate-600">
                  <span className="font-medium">Sequence:</span> {lastSSEEvent.sequence}
                </div>
              )}
            </div>
          </div>
          <div className="p-6">
            <pre className="bg-slate-50 border border-slate-200 rounded-lg p-4 overflow-x-auto text-xs font-mono text-slate-800">
              {JSON.stringify(lastSSEEvent, null, 2)}
            </pre>
          </div>
        </div>
      )}
    </div>
  );
}

// Helper function to render cell values
function renderValue(value: any): string {
  if (value === null || value === undefined) {
    return 'null';
  }

  if (typeof value === 'object') {
    try {
      return JSON.stringify(value, null, 2);
    } catch {
      return String(value);
    }
  }

  return String(value);
}
