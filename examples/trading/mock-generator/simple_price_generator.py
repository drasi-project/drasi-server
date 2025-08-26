#!/usr/bin/env python3

# Copyright 2025 The Drasi Authors.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http:#www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

"""
Simple Stock Price Generator for Testing
Sends price updates to Drasi HTTP Source using direct format
"""

import json
import random
import time
import requests
import sys
from datetime import datetime

def generate_price_update(symbol, base_price, volatility=0.02):
    """Generate a realistic price update"""
    change = random.gauss(0, volatility) * base_price
    new_price = max(base_price + change, 1.0)
    
    # Calculate previous close (simulated)
    previous_close = base_price * (1 + random.uniform(-0.05, 0.05))
    
    # Generate volume
    base_volume = random.randint(1000000, 50000000)
    volume = int(base_volume * (1 + random.uniform(-0.3, 0.3)))
    
    return {
        "symbol": symbol,
        "price": round(new_price, 2),
        "previous_close": round(previous_close, 2),
        "volume": volume,
        "timestamp": datetime.now().isoformat()
    }

def send_price_to_http_source(http_url, source_id, price_data):
    """Send price update to HTTP source using direct format"""
    
    # Create the direct format event
    event = {
        "operation": "update",
        "element": {
            "type": "node",
            "id": f"price_{price_data['symbol']}",
            "labels": ["stock_prices"],
            "properties": price_data
        },
        "timestamp": int(time.time() * 1_000_000_000)  # nanoseconds
    }
    
    url = f"{http_url}/sources/{source_id}/events"
    
    try:
        response = requests.post(url, json=event)
        if response.status_code == 200:
            print(f"✓ Sent price update for {price_data['symbol']}: ${price_data['price']}")
        else:
            print(f"✗ Failed to send {price_data['symbol']}: {response.status_code} - {response.text}")
    except Exception as e:
        print(f"✗ Error sending {price_data['symbol']}: {e}")

def main():
    # Configuration
    http_source_url = "http://localhost:9000"
    source_id = "price-feed"
    
    # Stock symbols with base prices
    stocks = {
        "AAPL": 175.50,
        "MSFT": 405.25, 
        "GOOGL": 140.75,
        "TSLA": 250.30,
        "NVDA": 880.25,
        "META": 485.50,
        "AMZN": 178.90,
        "AMD": 165.30,
        "INTC": 42.80,
        "JPM": 195.80,
        "BAC": 34.25,
        "WFC": 48.90,
        "GS": 425.60,
        "V": 280.45,
        "JNJ": 155.20,
        "PFE": 28.75,
        "UNH": 525.30,
        "CVS": 72.45,
        "XOM": 105.80,
        "CVX": 145.60,
        "DIS": 92.15,
        "NKE": 98.40,
        "BA": 215.70,
        "CAT": 285.30
    }
    
    print(f"Starting simple price generator...")
    print(f"HTTP Source URL: {http_source_url}")
    print(f"Source ID: {source_id}")
    print(f"Generating prices for {len(stocks)} stocks")
    print("-" * 50)
    
    # Send initial prices for all stocks
    print("Sending initial prices...")
    for symbol, base_price in stocks.items():
        price_data = generate_price_update(symbol, base_price)
        send_price_to_http_source(http_source_url, source_id, price_data)
        time.sleep(0.1)  # Small delay between initial updates
    
    print("-" * 50)
    print("Starting continuous price updates (Ctrl+C to stop)...")
    
    # Continuously generate price updates
    try:
        while True:
            # Pick a random subset of stocks to update
            num_updates = random.randint(3, 8)
            symbols_to_update = random.sample(list(stocks.keys()), num_updates)
            
            for symbol in symbols_to_update:
                # Update the base price slightly for next iteration
                current_price = stocks[symbol]
                stocks[symbol] = current_price * (1 + random.uniform(-0.001, 0.001))
                
                # Generate and send update
                price_data = generate_price_update(symbol, stocks[symbol])
                send_price_to_http_source(http_source_url, source_id, price_data)
            
            # Wait before next batch
            time.sleep(random.uniform(1, 3))
            
    except KeyboardInterrupt:
        print("\n" + "-" * 50)
        print("Price generator stopped")

if __name__ == "__main__":
    main()