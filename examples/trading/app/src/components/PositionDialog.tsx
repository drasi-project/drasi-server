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

export interface PositionFormData {
  id?: number;
  symbol: string;
  name: string;
  quantity: number;
  purchasePrice: number;
  purchaseDate: string;
}

interface PositionDialogProps {
  isOpen: boolean;
  mode: 'add' | 'edit';
  position?: PositionFormData;
  availableStocks: Stock[];
  isSubmitting: boolean;
  onSubmit: (data: PositionFormData) => Promise<void>;
  onCancel: () => void;
}

interface ValidationErrors {
  symbol?: string;
  quantity?: string;
  purchasePrice?: string;
  purchaseDate?: string;
}

export const PositionDialog: React.FC<PositionDialogProps> = ({
  isOpen,
  mode,
  position,
  availableStocks,
  isSubmitting,
  onSubmit,
  onCancel
}) => {
  const [symbol, setSymbol] = useState('');
  const [name, setName] = useState('');
  const [quantity, setQuantity] = useState('');
  const [purchasePrice, setPurchasePrice] = useState('');
  const [purchaseDate, setPurchaseDate] = useState(() => new Date().toISOString().split('T')[0]);
  const [errors, setErrors] = useState<ValidationErrors>({});
  const [submitError, setSubmitError] = useState<string | null>(null);

  // Reset form when dialog opens or position changes
  useEffect(() => {
    if (isOpen) {
      if (mode === 'edit' && position) {
        setSymbol(position.symbol);
        setName(position.name);
        setQuantity(position.quantity.toString());
        setPurchasePrice(position.purchasePrice.toString());
        setPurchaseDate(position.purchaseDate || new Date().toISOString().split('T')[0]);
      } else {
        // Add mode - reset to defaults
        if (availableStocks.length > 0) {
          setSymbol(availableStocks[0].symbol);
          setName(availableStocks[0].name);
        } else {
          setSymbol('');
          setName('');
        }
        setQuantity('');
        setPurchasePrice('');
        setPurchaseDate(new Date().toISOString().split('T')[0]);
      }
      setErrors({});
      setSubmitError(null);
    }
  }, [isOpen, mode, position, availableStocks]);

  // Update name when symbol changes in add mode
  const handleSymbolChange = (newSymbol: string) => {
    setSymbol(newSymbol);
    const stock = availableStocks.find(s => s.symbol === newSymbol);
    if (stock) {
      setName(stock.name);
    }
    // Clear symbol error when user makes a selection
    if (errors.symbol) {
      setErrors(prev => ({ ...prev, symbol: undefined }));
    }
  };

  const validate = (): boolean => {
    const newErrors: ValidationErrors = {};

    // Symbol validation (only for add mode)
    if (mode === 'add' && !symbol) {
      newErrors.symbol = 'Please select a stock';
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

    // Purchase price validation
    const price = parseFloat(purchasePrice);
    if (!purchasePrice) {
      newErrors.purchasePrice = 'Purchase price is required';
    } else if (isNaN(price)) {
      newErrors.purchasePrice = 'Price must be a number';
    } else if (price <= 0) {
      newErrors.purchasePrice = 'Price must be greater than 0';
    }

    // Purchase date validation
    if (!purchaseDate) {
      newErrors.purchaseDate = 'Purchase date is required';
    } else {
      const date = new Date(purchaseDate);
      const today = new Date();
      today.setHours(23, 59, 59, 999);
      if (isNaN(date.getTime())) {
        newErrors.purchaseDate = 'Invalid date';
      } else if (date > today) {
        newErrors.purchaseDate = 'Purchase date cannot be in the future';
      }
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
      await onSubmit({
        id: position?.id,
        symbol,
        name,
        quantity: parseInt(quantity),
        purchasePrice: parseFloat(purchasePrice),
        purchaseDate
      });
    } catch (err) {
      setSubmitError(err instanceof Error ? err.message : 'An error occurred');
    }
  };

  // Clear field error when user types
  const handleQuantityChange = (value: string) => {
    setQuantity(value);
    if (errors.quantity) {
      setErrors(prev => ({ ...prev, quantity: undefined }));
    }
  };

  const handlePriceChange = (value: string) => {
    setPurchasePrice(value);
    if (errors.purchasePrice) {
      setErrors(prev => ({ ...prev, purchasePrice: undefined }));
    }
  };

  const handleDateChange = (value: string) => {
    setPurchaseDate(value);
    if (errors.purchaseDate) {
      setErrors(prev => ({ ...prev, purchaseDate: undefined }));
    }
  };

  if (!isOpen) return null;

  const isAddMode = mode === 'add';
  const title = isAddMode ? 'Add Position' : 'Edit Position';
  const submitLabel = isAddMode ? 'Add' : 'Save';
  const submittingLabel = isAddMode ? 'Adding...' : 'Saving...';

  return (
    <BaseDialog
      isOpen={isOpen}
      onClose={onCancel}
      title={title}
      footer={
        <>
          <DialogButton onClick={onCancel} disabled={isSubmitting}>
            Cancel
          </DialogButton>
          <DialogButton
            onClick={handleSubmit}
            disabled={isSubmitting || (isAddMode && availableStocks.length === 0)}
            variant="primary"
          >
            {isSubmitting ? submittingLabel : submitLabel}
          </DialogButton>
        </>
      }
    >
      {/* Stock selector (add mode) or stock info (edit mode) */}
      {isAddMode ? (
        availableStocks.length === 0 ? (
          <p className="text-gray-400 mb-4">No more stocks available to add.</p>
        ) : (
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
        )
      ) : (
        <div className="bg-trading-bg rounded p-3 mb-4">
          <div className="text-lg font-bold">{symbol}</div>
          <div className="text-sm text-gray-400">{name}</div>
        </div>
      )}

      {/* Only show form fields if stocks are available (add mode) or always (edit mode) */}
      {(!isAddMode || availableStocks.length > 0) && (
        <>
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

          {/* Purchase Price */}
          <div className="mb-4">
            <label className="block text-sm text-gray-400 mb-2">Purchase Price ($)</label>
            <input
              type="number"
              step="0.01"
              value={purchasePrice}
              onChange={(e) => handlePriceChange(e.target.value)}
              placeholder="e.g., 150.00"
              className={`w-full bg-trading-bg border rounded p-2 text-white ${
                errors.purchasePrice ? 'border-red-500' : 'border-trading-border'
              }`}
            />
            {errors.purchasePrice && (
              <p className="text-red-400 text-sm mt-1">{errors.purchasePrice}</p>
            )}
          </div>

          {/* Purchase Date */}
          <div className="mb-4">
            <label className="block text-sm text-gray-400 mb-2">Purchase Date</label>
            <input
              type="date"
              value={purchaseDate}
              onChange={(e) => handleDateChange(e.target.value)}
              className={`w-full bg-trading-bg border rounded p-2 text-white ${
                errors.purchaseDate ? 'border-red-500' : 'border-trading-border'
              }`}
            />
            {errors.purchaseDate && (
              <p className="text-red-400 text-sm mt-1">{errors.purchaseDate}</p>
            )}
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
