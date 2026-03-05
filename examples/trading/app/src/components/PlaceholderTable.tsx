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

import React from 'react';
import clsx from 'clsx';

interface PlaceholderTableProps {
  title: string;
  message?: string;
  height?: string;
  className?: string;
}

/**
 * PlaceholderTable - A placeholder component matching the QueryTable visual style.
 * Use this for planned features or empty states.
 */
export const PlaceholderTable: React.FC<PlaceholderTableProps> = ({
  title,
  message = 'Coming soon',
  height = 'h-[400px]',
  className,
}) => {
  return (
    <div className={clsx(
      "bg-trading-card rounded-lg border border-trading-border flex flex-col",
      height,
      className
    )}>
      {/* Header */}
      <div className="flex justify-between items-center p-6 pb-4 flex-shrink-0">
        <h2 className="text-xl font-bold">{title}</h2>
      </div>

      {/* Empty state content */}
      <div className="flex-1 flex items-center justify-center px-6 pb-6">
        <div className="text-center">
          <svg
            className="w-12 h-12 mx-auto mb-3 text-gray-600"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={1.5}
              d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2"
            />
          </svg>
          <p className="text-gray-500">{message}</p>
        </div>
      </div>
    </div>
  );
};
