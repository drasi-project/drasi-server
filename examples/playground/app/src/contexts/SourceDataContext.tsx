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

import React, { createContext, useContext, useState, ReactNode } from 'react';

interface SourceDataContextType {
  // Store data for each source and type: sourceId -> typeLabel -> data array
  sourceData: Map<string, Map<string, any[]>>;
  // Store original data: sourceId -> typeLabel -> recordId -> data
  originalSourceData: Map<string, Map<string, Map<string, any>>>;

  // Methods to manage data
  getSourceTypeData: (sourceId: string, typeLabel: string) => any[];
  setSourceTypeData: (sourceId: string, typeLabel: string, data: any[]) => void;
  getOriginalData: (sourceId: string, typeLabel: string) => Map<string, any>;
  setOriginalData: (sourceId: string, typeLabel: string, data: Map<string, any>) => void;
  getSourceTypes: (sourceId: string) => string[];
  clearSourceTypeData: (sourceId: string, typeLabel: string) => void;
  clearSourceData: (sourceId: string) => void;
  clearAllData: () => void;
}

const SourceDataContext = createContext<SourceDataContextType | undefined>(undefined);

export function SourceDataProvider({ children }: { children: ReactNode }) {
  // Map of sourceId -> typeLabel -> array of current data
  const [sourceData, setSourceDataMap] = useState<Map<string, Map<string, any[]>>>(new Map());
  // Map of sourceId -> typeLabel -> Map of recordId -> original data
  const [originalSourceData, setOriginalSourceDataMap] = useState<Map<string, Map<string, Map<string, any>>>>(new Map());

  const getSourceTypeData = (sourceId: string, typeLabel: string): any[] => {
    const sourceMap = sourceData.get(sourceId);
    if (!sourceMap) return [];
    return sourceMap.get(typeLabel) || [];
  };

  const setSourceTypeData = (sourceId: string, typeLabel: string, data: any[]) => {
    setSourceDataMap(prev => {
      const newMap = new Map(prev);
      if (!newMap.has(sourceId)) {
        newMap.set(sourceId, new Map());
      }
      const sourceMap = newMap.get(sourceId)!;
      sourceMap.set(typeLabel, data);
      return newMap;
    });
  };

  const getOriginalData = (sourceId: string, typeLabel: string): Map<string, any> => {
    const sourceMap = originalSourceData.get(sourceId);
    if (!sourceMap) return new Map();
    return sourceMap.get(typeLabel) || new Map();
  };

  const setOriginalData = (sourceId: string, typeLabel: string, data: Map<string, any>) => {
    setOriginalSourceDataMap(prev => {
      const newMap = new Map(prev);
      if (!newMap.has(sourceId)) {
        newMap.set(sourceId, new Map());
      }
      const sourceMap = newMap.get(sourceId)!;
      sourceMap.set(typeLabel, data);
      return newMap;
    });
  };

  const getSourceTypes = (sourceId: string): string[] => {
    const sourceMap = sourceData.get(sourceId);
    if (!sourceMap) return [];
    return Array.from(sourceMap.keys());
  };

  const clearSourceTypeData = (sourceId: string, typeLabel: string) => {
    setSourceDataMap(prev => {
      const newMap = new Map(prev);
      const sourceMap = newMap.get(sourceId);
      if (sourceMap) {
        sourceMap.delete(typeLabel);
      }
      return newMap;
    });
    setOriginalSourceDataMap(prev => {
      const newMap = new Map(prev);
      const sourceMap = newMap.get(sourceId);
      if (sourceMap) {
        sourceMap.delete(typeLabel);
      }
      return newMap;
    });
  };

  const clearSourceData = (sourceId: string) => {
    setSourceDataMap(prev => {
      const newMap = new Map(prev);
      newMap.delete(sourceId);
      return newMap;
    });
    setOriginalSourceDataMap(prev => {
      const newMap = new Map(prev);
      newMap.delete(sourceId);
      return newMap;
    });
  };

  const clearAllData = () => {
    setSourceDataMap(new Map());
    setOriginalSourceDataMap(new Map());
  };

  return (
    <SourceDataContext.Provider
      value={{
        sourceData,
        originalSourceData,
        getSourceTypeData,
        setSourceTypeData,
        getOriginalData,
        setOriginalData,
        getSourceTypes,
        clearSourceTypeData,
        clearSourceData,
        clearAllData,
      }}
    >
      {children}
    </SourceDataContext.Provider>
  );
}

export function useSourceData() {
  const context = useContext(SourceDataContext);
  if (context === undefined) {
    throw new Error('useSourceData must be used within a SourceDataProvider');
  }
  return context;
}