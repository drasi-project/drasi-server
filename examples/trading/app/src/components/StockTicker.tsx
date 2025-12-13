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

import React, { useEffect, useState, useRef, useCallback } from 'react';
import { useQuery } from '@drasi/react';
import './StockTicker.css';

interface TickerItem {
  id: string;
  symbol: string;
  price: number;
  changePercent: number;
  timestamp: number;
  position: number; // Track position for smooth scrolling
}

const StockTicker: React.FC = () => {
  const { data: priceData } = useQuery<any>('price-ticker-query');
  const [tickerItems, setTickerItems] = useState<TickerItem[]>([]);
  const containerRef = useRef<HTMLDivElement>(null);
  const animationFrameRef = useRef<number>();
  const lastUpdateRef = useRef<number>(0);
  const itemPositionsRef = useRef<Map<string, number>>(new Map());
  
  // Track which symbols we've seen to detect actual changes
  const seenDataRef = useRef<Map<string, { price: number; changePercent: number }>>(new Map());

  // Add new items to the ticker when price data changes
  useEffect(() => {
    if (!priceData || priceData.length === 0) return;

    const now = Date.now();
    // Throttle updates to prevent too frequent additions
    if (now - lastUpdateRef.current < 500) return;
    lastUpdateRef.current = now;

    const newItems: TickerItem[] = [];
    
    priceData.forEach((item: any) => {
      const price = typeof item.price === 'number' ? item.price : parseFloat(item.price) || 0;
      const changePercent = typeof item.changePercent === 'number' ? item.changePercent : parseFloat(item.changePercent) || 0;
      
      const previousData = seenDataRef.current.get(item.symbol);
      
      // Only add if this is new data or price has changed
      if (!previousData || Math.abs(previousData.price - price) > 0.001) {
        const id = `${item.symbol}-${now}-${Math.random()}`;
        
        // Calculate starting position (off-screen to the right)
        // Add proper spacing between consecutive items (250px gap)
        const positions = Array.from(itemPositionsRef.current.values());
        const lastItemPos = positions.length > 0 ? Math.max(...positions) : window.innerWidth;
        const startPosition = lastItemPos + 250; // Increased spacing between items
        
        newItems.push({
          id,
          symbol: item.symbol,
          price,
          changePercent,
          timestamp: now,
          position: startPosition
        });
        
        itemPositionsRef.current.set(id, startPosition);
        seenDataRef.current.set(item.symbol, { price, changePercent });
      }
    });
    
    if (newItems.length > 0) {
      setTickerItems(prev => {
        // Remove items that have scrolled off screen (past -500px)
        const filtered = prev.filter(item => {
          const pos = itemPositionsRef.current.get(item.id) || 0;
          return pos > -500;
        });
        
        // Add new items
        return [...filtered, ...newItems];
      });
    }
  }, [priceData]);

  // Animation loop for smooth scrolling
  const animate = useCallback(() => {
    const speed = 1.5; // Pixels per frame
    
    setTickerItems(prev => {
      return prev.map(item => {
        const currentPos = itemPositionsRef.current.get(item.id) || 0;
        const newPos = currentPos - speed;
        itemPositionsRef.current.set(item.id, newPos);
        
        return {
          ...item,
          position: newPos
        };
      }).filter(item => item.position > -500); // Remove items that have scrolled off
    });
    
    animationFrameRef.current = requestAnimationFrame(animate);
  }, []);

  // Start animation
  useEffect(() => {
    animationFrameRef.current = requestAnimationFrame(animate);
    
    return () => {
      if (animationFrameRef.current) {
        cancelAnimationFrame(animationFrameRef.current);
      }
    };
  }, [animate]);

  if (tickerItems.length === 0) {
    return (
      <div className="stock-ticker">
        <div className="ticker-container">
          <div className="ticker-content-static">
            <span className="ticker-placeholder">Waiting for price updates...</span>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="stock-ticker">
      <div className="ticker-container" ref={containerRef}>
        <div className="ticker-content-smooth">
          {tickerItems.map((item) => (
            <div 
              key={item.id} 
              className="ticker-item-absolute"
              style={{
                transform: `translateX(${item.position}px)`,
              }}
            >
              <span className="ticker-symbol">{item.symbol}</span>
              <span className="ticker-price">${item.price.toFixed(2)}</span>
              <span className={`ticker-change ${item.changePercent >= 0 ? 'positive' : 'negative'}`}>
                {item.changePercent >= 0 ? '▲' : '▼'}
                {Math.abs(item.changePercent).toFixed(2)}%
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};

export default StockTicker;