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

import React, { useState, useEffect } from 'react';
import { Stock } from '@/services/TradingApi';
import { BaseDialog, DialogButton } from './shared';

export interface OrderFormData {
  symbol: string;
  orderType: 'buy' | 'sell';
  targetPrice: number;
  quantity: number;
  expiresAt: string; // ISO timestamp calculated from expiresInSeconds
  staleDuration: number; // seconds - half of expireDuration, rounded down
  expireDuration: number; // seconds - user-entered expiry time
}

interface OrderDialogProps {
  isOpen: boolean;
  availableStocks: Stock[];
  isSubmitting: boolean;
  onSubmit: (data: OrderFormData) => Promise<void>;
  onCancel: () => void;
}

interface ValidationErrors {
  symbol?: string;
  targetPrice?: string;
  quantity?: string;
  expiresIn?: string;
}

export const OrderDialog: React.FC<OrderDialogProps> = ({
  isOpen,
  availableStocks,
  isSubmitting,
  onSubmit,
  onCancel
}) => {
  const [symbol, setSymbol] = useState('');
  const [orderType, setOrderType] = useState<'buy' | 'sell'>('buy');
  const [targetPrice, setTargetPrice] = useState('');
  const [quantity, setQuantity] = useState('');
  const [expiresIn, setExpiresIn] = useState('60'); // Default 60 seconds
  const [errors, setErrors] = useState<ValidationErrors>({});
  const [submitError, setSubmitError] = useState<string | null>(null);

  // Reset form when dialog opens
  useEffect(() => {
    if (isOpen) {
      if (availableStocks.length > 0) {
        setSymbol(availableStocks[0].symbol);
      } else {
        setSymbol('');
      }
      setOrderType('buy');
      setTargetPrice('');
      setQuantity('');
      setExpiresIn('60');
      setErrors({});
      setSubmitError(null);
    }
  }, [isOpen, availableStocks]);

  const handleSymbolChange = (newSymbol: string) => {
    setSymbol(newSymbol);
    if (errors.symbol) {
      setErrors(prev => ({ ...prev, symbol: undefined }));
    }
  };

  const validate = (): boolean => {
    const newErrors: ValidationErrors = {};

    // Symbol validation
    if (!symbol) {
      newErrors.symbol = 'Please select a stock';
    }

    // Target price validation
    const price = parseFloat(targetPrice);
    if (!targetPrice) {
      newErrors.targetPrice = 'Target price is required';
    } else if (isNaN(price)) {
      newErrors.targetPrice = 'Price must be a number';
    } else if (price <= 0) {
      newErrors.targetPrice = 'Price must be greater than 0';
    }

    // Quantity validation
    const qty = parseInt(quantity);
    if (!quantity) {
      newErrors.quantity = 'Quantity is required';
    } else if (isNaN(qty)) {
      newErrors.quantity = 'Quantity must be a number';
    } else if (qty <= 0) {
      newErrors.quantity = 'Quantity must be greater than 0';
    } else if (!Number.isInteger(qty)) {
      newErrors.quantity = 'Quantity must be a whole number';
    }

    // Expires in validation
    const seconds = parseInt(expiresIn);
    if (!expiresIn) {
      newErrors.expiresIn = 'Expiration time is required';
    } else if (isNaN(seconds)) {
      newErrors.expiresIn = 'Must be a number';
    } else if (seconds < 10) {
      newErrors.expiresIn = 'Must be at least 10 seconds';
    } else if (seconds > 3600) {
      newErrors.expiresIn = 'Cannot exceed 3600 seconds (1 hour)';
    }

    setErrors(newErrors);
    return Object.keys(newErrors).length === 0;
  };

  const handleSubmit = async () => {
    setSubmitError(null);
    
    if (!validate()) {
      return;
    }

    try {
      const expiresInSeconds = parseInt(expiresIn);
      // Calculate expiration timestamp by adding expiresIn seconds to current time
      const expiresAt = new Date(Date.now() + expiresInSeconds * 1000).toISOString();
      
      await onSubmit({
        symbol,
        orderType,
        targetPrice: parseFloat(targetPrice),
        quantity: parseInt(quantity),
        expiresAt,
        staleDuration: Math.floor(expiresInSeconds / 2),
        expireDuration: expiresInSeconds,
      });
    } catch (err) {
      setSubmitError(err instanceof Error ? err.message : 'An error occurred');
    }
  };

  // Clear field error when user types
  const handlePriceChange = (value: string) => {
    setTargetPrice(value);
    if (errors.targetPrice) {
      setErrors(prev => ({ ...prev, targetPrice: undefined }));
    }
  };

  const handleQuantityChange = (value: string) => {
    setQuantity(value);
    if (errors.quantity) {
      setErrors(prev => ({ ...prev, quantity: undefined }));
    }
  };

  const handleExpiresInChange = (value: string) => {
    setExpiresIn(value);
    if (errors.expiresIn) {
      setErrors(prev => ({ ...prev, expiresIn: undefined }));
    }
  };

  if (!isOpen) return null;

  return (
    <BaseDialog
      isOpen={isOpen}
      onClose={onCancel}
      title="New Limit Order"
      footer={
        <>
          <DialogButton onClick={onCancel} disabled={isSubmitting}>
            Cancel
          </DialogButton>
          <DialogButton
            onClick={handleSubmit}
            disabled={isSubmitting || availableStocks.length === 0}
            variant="primary"
          >
            {isSubmitting ? 'Creating...' : 'Create Order'}
          </DialogButton>
        </>
      }
    >
      {availableStocks.length === 0 ? (
        <p className="text-gray-400 mb-4">No stocks available.</p>
      ) : (
        <>
          {/* Stock selector */}
          <div className="mb-4">
            <label className="block text-sm text-gray-400 mb-2">Stock</label>
            <select
              value={symbol}
              onChange={(e) => handleSymbolChange(e.target.value)}
              className={`w-full bg-trading-bg border rounded p-2 text-white ${
                errors.symbol ? 'border-red-500' : 'border-trading-border'
              }`}
            >
              {availableStocks.map(stock => (
                <option key={stock.symbol} value={stock.symbol}>
                  {stock.symbol} - {stock.name}
                </option>
              ))}
            </select>
            {errors.symbol && (
              <p className="text-red-400 text-sm mt-1">{errors.symbol}</p>
            )}
          </div>

          {/* Order Type */}
          <div className="mb-4">
            <label className="block text-sm text-gray-400 mb-2">Order Type</label>
            <div className="flex gap-2">
              <button
                type="button"
                onClick={() => setOrderType('buy')}
                className={`flex-1 py-2 px-4 rounded font-medium transition-colors ${
                  orderType === 'buy'
                    ? 'bg-trading-green text-white'
                    : 'bg-trading-bg border border-trading-border text-gray-400 hover:border-trading-green'
                }`}
              >
                Buy
              </button>
              <button
                type="button"
                onClick={() => setOrderType('sell')}
                className={`flex-1 py-2 px-4 rounded font-medium transition-colors ${
                  orderType === 'sell'
                    ? 'bg-trading-red text-white'
                    : 'bg-trading-bg border border-trading-border text-gray-400 hover:border-trading-red'
                }`}
              >
                Sell
              </button>
            </div>
            <p className="text-xs text-gray-500 mt-1">
              {orderType === 'buy' 
                ? 'Buy when price drops to target' 
                : 'Sell when price rises to target'}
            </p>
          </div>

          {/* Target Price */}
          <div className="mb-4">
            <label className="block text-sm text-gray-400 mb-2">Target Price ($)</label>
            <input
              type="number"
              step="0.01"
              value={targetPrice}
              onChange={(e) => handlePriceChange(e.target.value)}
              placeholder="e.g., 150.00"
              className={`w-full bg-trading-bg border rounded p-2 text-white ${
                errors.targetPrice ? 'border-red-500' : 'border-trading-border'
              }`}
            />
            {errors.targetPrice && (
              <p className="text-red-400 text-sm mt-1">{errors.targetPrice}</p>
            )}
          </div>

          {/* Quantity */}
          <div className="mb-4">
            <label className="block text-sm text-gray-400 mb-2">Quantity</label>
            <input
              type="number"
              value={quantity}
              onChange={(e) => handleQuantityChange(e.target.value)}
              placeholder="e.g., 100"
              className={`w-full bg-trading-bg border rounded p-2 text-white ${
                errors.quantity ? 'border-red-500' : 'border-trading-border'
              }`}
            />
            {errors.quantity && (
              <p className="text-red-400 text-sm mt-1">{errors.quantity}</p>
            )}
          </div>

          {/* Expires In (seconds) */}
          <div className="mb-4">
            <label className="block text-sm text-gray-400 mb-2">Expires In (seconds)</label>
            <input
              type="number"
              value={expiresIn}
              onChange={(e) => handleExpiresInChange(e.target.value)}
              placeholder="e.g., 60"
              min="10"
              max="3600"
              className={`w-full bg-trading-bg border rounded p-2 text-white ${
                errors.expiresIn ? 'border-red-500' : 'border-trading-border'
              }`}
            />
            {errors.expiresIn && (
              <p className="text-red-400 text-sm mt-1">{errors.expiresIn}</p>
            )}
            <p className="text-xs text-gray-500 mt-1">
              Order will expire after this many seconds (demonstrates drasi.trueLater)
            </p>
          </div>
        </>
      )}

      {/* Submit error */}
      {submitError && (
        <div className="p-2 bg-red-900/30 border border-red-500/50 rounded text-sm text-red-400">
          {submitError}
        </div>
      )}
    </BaseDialog>
  );
};
