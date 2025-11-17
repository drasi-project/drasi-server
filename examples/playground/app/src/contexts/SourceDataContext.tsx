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
  // Store data for each source by sourceId
  sourceData: Map<string, any[]>;
  originalSourceData: Map<string, Map<string, any>>;

  // Methods to manage data
  getSourceData: (sourceId: string) => any[];
  setSourceData: (sourceId: string, data: any[]) => void;
  getOriginalData: (sourceId: string) => Map<string, any>;
  setOriginalData: (sourceId: string, data: Map<string, any>) => void;
  clearSourceData: (sourceId: string) => void;
  clearAllData: () => void;
}

const SourceDataContext = createContext<SourceDataContextType | undefined>(undefined);

export function SourceDataProvider({ children }: { children: ReactNode }) {
  // Map of sourceId -> array of current data
  const [sourceData, setSourceDataMap] = useState<Map<string, any[]>>(new Map());
  // Map of sourceId -> Map of recordId -> original data
  const [originalSourceData, setOriginalSourceDataMap] = useState<Map<string, Map<string, any>>>(new Map());

  const getSourceData = (sourceId: string): any[] => {
    return sourceData.get(sourceId) || [];
  };

  const setSourceData = (sourceId: string, data: any[]) => {
    setSourceDataMap(prev => {
      const newMap = new Map(prev);
      newMap.set(sourceId, data);
      return newMap;
    });
  };

  const getOriginalData = (sourceId: string): Map<string, any> => {
    return originalSourceData.get(sourceId) || new Map();
  };

  const setOriginalData = (sourceId: string, data: Map<string, any>) => {
    setOriginalSourceDataMap(prev => {
      const newMap = new Map(prev);
      newMap.set(sourceId, data);
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
        getSourceData,
        setSourceData,
        getOriginalData,
        setOriginalData,
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