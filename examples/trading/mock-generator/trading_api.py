#!/usr/bin/env python3

# Copyright 2025 The Drasi Authors.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

"""
Trading API Server for CRUD operations on watchlist and portfolio.
Demonstrates end-to-end data flow through Drasi:
  UI Action → Database Write → PostgreSQL CDC → Drasi Query → SSE → UI Update
"""

import os
from datetime import datetime, date
from flask import Flask, jsonify, request
from flask.json.provider import DefaultJSONProvider
from flask_cors import CORS
import psycopg2
from psycopg2.extras import RealDictCursor

class CustomJSONProvider(DefaultJSONProvider):
    """Custom JSON provider to handle datetime serialization."""
    def default(self, obj):
        if isinstance(obj, datetime):
            return obj.isoformat()
        if isinstance(obj, date):
            return obj.isoformat()
        return super().default(obj)

app = Flask(__name__)
app.json = CustomJSONProvider(app)
CORS(app)

# Database configuration from environment or defaults
DB_CONFIG = {
    'host': os.getenv('POSTGRES_HOST', 'localhost'),
    'port': int(os.getenv('POSTGRES_PORT', '5632')),
    'database': os.getenv('POSTGRES_DB', 'trading_demo'),
    'user': os.getenv('POSTGRES_USER', 'drasi_user'),
    'password': os.getenv('POSTGRES_PASSWORD', 'drasi_password')
}

def get_db_connection():
    """Get a database connection."""
    return psycopg2.connect(**DB_CONFIG, cursor_factory=RealDictCursor)

# ============================================================================
# Stocks API - Read-only list of available stocks
# ============================================================================

@app.route('/api/stocks', methods=['GET'])
def list_stocks():
    """List all available stocks for dropdowns."""
    try:
        conn = get_db_connection()
        cur = conn.cursor()
        cur.execute('''
            SELECT symbol, name, sector, industry 
            FROM stocks 
            ORDER BY symbol
        ''')
        stocks = cur.fetchall()
        cur.close()
        conn.close()
        return jsonify({'success': True, 'data': stocks})
    except Exception as e:
        return jsonify({'success': False, 'error': str(e)}), 500

# ============================================================================
# Watchlist API - CRUD for watchlist table
# ============================================================================

@app.route('/api/watchlist', methods=['GET'])
def list_watchlist():
    """List all watchlist items for the demo user."""
    try:
        conn = get_db_connection()
        cur = conn.cursor()
        cur.execute('''
            SELECT w.id, w.symbol, w.added_at, s.name, s.sector
            FROM watchlist w
            JOIN stocks s ON w.symbol = s.symbol
            WHERE w.user_id = 'demo_user'
            ORDER BY w.symbol
        ''')
        items = cur.fetchall()
        cur.close()
        conn.close()
        return jsonify({'success': True, 'data': items})
    except Exception as e:
        return jsonify({'success': False, 'error': str(e)}), 500

@app.route('/api/watchlist', methods=['POST'])
def add_to_watchlist():
    """Add a stock to the watchlist."""
    try:
        data = request.get_json()
        symbol = data.get('symbol')
        
        if not symbol:
            return jsonify({'success': False, 'error': 'Symbol is required'}), 400
        
        conn = get_db_connection()
        cur = conn.cursor()
        
        # Check if stock exists
        cur.execute('SELECT symbol FROM stocks WHERE symbol = %s', (symbol,))
        if not cur.fetchone():
            cur.close()
            conn.close()
            return jsonify({'success': False, 'error': f'Stock {symbol} not found'}), 404
        
        # Check if already in watchlist
        cur.execute('''
            SELECT id FROM watchlist 
            WHERE user_id = 'demo_user' AND symbol = %s
        ''', (symbol,))
        if cur.fetchone():
            cur.close()
            conn.close()
            return jsonify({'success': False, 'error': f'{symbol} already in watchlist'}), 409
        
        # Insert into watchlist
        cur.execute('''
            INSERT INTO watchlist (user_id, symbol)
            VALUES ('demo_user', %s)
            RETURNING id, symbol, added_at
        ''', (symbol,))
        
        new_item = cur.fetchone()
        conn.commit()
        cur.close()
        conn.close()
        
        return jsonify({'success': True, 'data': new_item}), 201
    except Exception as e:
        return jsonify({'success': False, 'error': str(e)}), 500

@app.route('/api/watchlist/<symbol>', methods=['DELETE'])
def remove_from_watchlist(symbol):
    """Remove a stock from the watchlist."""
    try:
        conn = get_db_connection()
        cur = conn.cursor()
        
        cur.execute('''
            DELETE FROM watchlist 
            WHERE user_id = 'demo_user' AND symbol = %s
            RETURNING id
        ''', (symbol,))
        
        deleted = cur.fetchone()
        conn.commit()
        cur.close()
        conn.close()
        
        if not deleted:
            return jsonify({'success': False, 'error': f'{symbol} not in watchlist'}), 404
        
        return jsonify({'success': True, 'message': f'{symbol} removed from watchlist'})
    except Exception as e:
        return jsonify({'success': False, 'error': str(e)}), 500

# ============================================================================
# Portfolio API - CRUD for portfolio table
# ============================================================================

@app.route('/api/portfolio', methods=['GET'])
def list_portfolio():
    """List all portfolio positions for the demo user."""
    try:
        conn = get_db_connection()
        cur = conn.cursor()
        cur.execute('''
            SELECT p.id, p.symbol, p.quantity, p.purchase_price, p.purchase_date,
                   s.name, s.sector
            FROM portfolio p
            JOIN stocks s ON p.symbol = s.symbol
            WHERE p.user_id = 'demo_user'
            ORDER BY p.symbol
        ''')
        positions = cur.fetchall()
        cur.close()
        conn.close()
        return jsonify({'success': True, 'data': positions})
    except Exception as e:
        return jsonify({'success': False, 'error': str(e)}), 500

@app.route('/api/portfolio', methods=['POST'])
def add_position():
    """Add a new portfolio position."""
    try:
        data = request.get_json()
        symbol = data.get('symbol')
        quantity = data.get('quantity')
        purchase_price = data.get('purchasePrice')
        purchase_date_str = data.get('purchaseDate')
        
        if not all([symbol, quantity, purchase_price]):
            return jsonify({
                'success': False, 
                'error': 'symbol, quantity, and purchasePrice are required'
            }), 400
        
        # Parse purchase date or default to now
        if purchase_date_str:
            purchase_date = datetime.fromisoformat(purchase_date_str.replace('Z', '+00:00'))
        else:
            purchase_date = datetime.now()
        
        conn = get_db_connection()
        cur = conn.cursor()
        
        # Check if stock exists
        cur.execute('SELECT symbol FROM stocks WHERE symbol = %s', (symbol,))
        if not cur.fetchone():
            cur.close()
            conn.close()
            return jsonify({'success': False, 'error': f'Stock {symbol} not found'}), 404
        
        # Insert position
        cur.execute('''
            INSERT INTO portfolio (user_id, symbol, quantity, purchase_price, purchase_date)
            VALUES ('demo_user', %s, %s, %s, %s)
            RETURNING id, symbol, quantity, purchase_price, purchase_date
        ''', (symbol, quantity, purchase_price, purchase_date))
        
        new_position = cur.fetchone()
        conn.commit()
        cur.close()
        conn.close()
        
        return jsonify({'success': True, 'data': new_position}), 201
    except Exception as e:
        return jsonify({'success': False, 'error': str(e)}), 500

@app.route('/api/portfolio/<int:position_id>', methods=['PUT'])
def update_position(position_id):
    """Update a portfolio position."""
    try:
        data = request.get_json()
        quantity = data.get('quantity')
        purchase_price = data.get('purchasePrice')
        purchase_date_str = data.get('purchaseDate')
        
        if quantity is None and purchase_price is None and purchase_date_str is None:
            return jsonify({
                'success': False, 
                'error': 'At least quantity, purchasePrice, or purchaseDate must be provided'
            }), 400
        
        conn = get_db_connection()
        cur = conn.cursor()
        
        # Build update query dynamically
        updates = []
        params = []
        if quantity is not None:
            updates.append('quantity = %s')
            params.append(quantity)
        if purchase_price is not None:
            updates.append('purchase_price = %s')
            params.append(purchase_price)
        if purchase_date_str is not None:
            updates.append('purchase_date = %s')
            purchase_date = datetime.fromisoformat(purchase_date_str.replace('Z', '+00:00'))
            params.append(purchase_date)
        
        params.extend([position_id])
        
        cur.execute(f'''
            UPDATE portfolio 
            SET {', '.join(updates)}
            WHERE id = %s AND user_id = 'demo_user'
            RETURNING id, symbol, quantity, purchase_price, purchase_date
        ''', params)
        
        updated = cur.fetchone()
        conn.commit()
        cur.close()
        conn.close()
        
        if not updated:
            return jsonify({'success': False, 'error': 'Position not found'}), 404
        
        return jsonify({'success': True, 'data': updated})
    except Exception as e:
        return jsonify({'success': False, 'error': str(e)}), 500

@app.route('/api/portfolio/<int:position_id>', methods=['DELETE'])
def delete_position(position_id):
    """Delete a portfolio position."""
    try:
        conn = get_db_connection()
        cur = conn.cursor()
        
        cur.execute('''
            DELETE FROM portfolio 
            WHERE id = %s AND user_id = 'demo_user'
            RETURNING id, symbol
        ''', (position_id,))
        
        deleted = cur.fetchone()
        conn.commit()
        cur.close()
        conn.close()
        
        if not deleted:
            return jsonify({'success': False, 'error': 'Position not found'}), 404
        
        return jsonify({
            'success': True, 
            'message': f'Position {deleted["symbol"]} deleted'
        })
    except Exception as e:
        return jsonify({'success': False, 'error': str(e)}), 500

# ============================================================================
# Limit Orders API - CRUD for limit_orders table
# ============================================================================

@app.route('/api/orders', methods=['GET'])
def list_orders():
    """List all limit orders for the demo user."""
    try:
        status_filter = request.args.get('status')  # Optional: filter by status
        
        conn = get_db_connection()
        cur = conn.cursor()
        
        if status_filter:
            cur.execute('''
                SELECT o.id, o.symbol, o.order_type, o.target_price, o.quantity,
                       o.status, o.created_at, o.filled_at, o.expires_at,
                       s.name, s.sector
                FROM limit_orders o
                JOIN stocks s ON o.symbol = s.symbol
                WHERE o.user_id = 'demo_user' AND o.status = %s
                ORDER BY o.created_at DESC
            ''', (status_filter,))
        else:
            cur.execute('''
                SELECT o.id, o.symbol, o.order_type, o.target_price, o.quantity,
                       o.status, o.created_at, o.filled_at, o.expires_at,
                       s.name, s.sector
                FROM limit_orders o
                JOIN stocks s ON o.symbol = s.symbol
                WHERE o.user_id = 'demo_user'
                ORDER BY o.created_at DESC
            ''')
        
        orders = cur.fetchall()
        cur.close()
        conn.close()
        return jsonify({'success': True, 'data': orders})
    except Exception as e:
        return jsonify({'success': False, 'error': str(e)}), 500

@app.route('/api/orders', methods=['POST'])
def create_order():
    """Create a new limit order."""
    try:
        data = request.get_json()
        symbol = data.get('symbol')
        order_type = data.get('orderType')  # 'buy' or 'sell'
        target_price = data.get('targetPrice')
        quantity = data.get('quantity')
        expires_at_str = data.get('expiresAt')  # ISO timestamp
        stale_duration = data.get('staleDuration')  # seconds (integer)
        expire_duration = data.get('expireDuration')  # seconds (integer)
        
        if not all([symbol, order_type, target_price, quantity]):
            return jsonify({
                'success': False, 
                'error': 'symbol, orderType, targetPrice, and quantity are required'
            }), 400
        
        if order_type not in ['buy', 'sell']:
            return jsonify({
                'success': False,
                'error': 'orderType must be "buy" or "sell"'
            }), 400
        
        # Parse expiration time (required)
        expires_at = None
        if expires_at_str:
            expires_at = datetime.fromisoformat(expires_at_str.replace('Z', '+00:00'))
        
        conn = get_db_connection()
        cur = conn.cursor()
        
        # Check if stock exists
        cur.execute('SELECT symbol FROM stocks WHERE symbol = %s', (symbol,))
        if not cur.fetchone():
            cur.close()
            conn.close()
            return jsonify({'success': False, 'error': f'Stock {symbol} not found'}), 404
        
        # Insert order
        cur.execute('''
            INSERT INTO limit_orders (user_id, symbol, order_type, target_price, quantity, expires_at, stale_duration, expire_duration)
            VALUES ('demo_user', %s, %s, %s, %s, %s, %s, %s)
            RETURNING id, symbol, order_type, target_price, quantity, status, created_at, expires_at, stale_duration, expire_duration
        ''', (symbol, order_type, target_price, quantity, expires_at, stale_duration, expire_duration))
        
        new_order = cur.fetchone()
        conn.commit()
        cur.close()
        conn.close()
        
        return jsonify({'success': True, 'data': new_order}), 201
    except Exception as e:
        return jsonify({'success': False, 'error': str(e)}), 500

@app.route('/api/orders/<int:order_id>', methods=['PUT'])
def update_order(order_id):
    """Update a limit order status."""
    try:
        data = request.get_json()
        status = data.get('status')
        
        if not status:
            return jsonify({
                'success': False, 
                'error': 'status is required'
            }), 400
        
        if status not in ['pending', 'stale', 'filled', 'expired', 'cancelled']:
            return jsonify({
                'success': False,
                'error': 'Invalid status value. Must be: pending, stale, filled, expired, cancelled'
            }), 400
        
        conn = get_db_connection()
        cur = conn.cursor()
        
        # Build update based on status
        if status == 'filled':
            cur.execute('''
                UPDATE limit_orders 
                SET status = %s, filled_at = CURRENT_TIMESTAMP
                WHERE id = %s AND user_id = 'demo_user'
                RETURNING id, symbol, order_type, target_price, quantity, status, filled_at
            ''', (status, order_id))
        else:
            cur.execute('''
                UPDATE limit_orders 
                SET status = %s
                WHERE id = %s AND user_id = 'demo_user'
                RETURNING id, symbol, order_type, target_price, quantity, status
            ''', (status, order_id))
        
        updated = cur.fetchone()
        conn.commit()
        cur.close()
        conn.close()
        
        if not updated:
            return jsonify({'success': False, 'error': 'Order not found'}), 404
        
        return jsonify({'success': True, 'data': updated})
    except Exception as e:
        return jsonify({'success': False, 'error': str(e)}), 500

@app.route('/api/orders/<int:order_id>', methods=['DELETE'])
def delete_order(order_id):
    """Delete (cancel) a limit order."""
    try:
        conn = get_db_connection()
        cur = conn.cursor()
        
        cur.execute('''
            DELETE FROM limit_orders 
            WHERE id = %s AND user_id = 'demo_user'
            RETURNING id, symbol
        ''', (order_id,))
        
        deleted = cur.fetchone()
        conn.commit()
        cur.close()
        conn.close()
        
        if not deleted:
            return jsonify({
                'success': False, 
                'error': 'Order not found or cannot be deleted (already filled/expired)'
            }), 404
        
        return jsonify({
            'success': True, 
            'message': f'Order {deleted["id"]} for {deleted["symbol"]} cancelled'
        })
    except Exception as e:
        return jsonify({'success': False, 'error': str(e)}), 500

# ============================================================================
# Health check
# ============================================================================

@app.route('/health', methods=['GET'])
def health():
    """Health check endpoint."""
    try:
        conn = get_db_connection()
        cur = conn.cursor()
        cur.execute('SELECT 1')
        cur.close()
        conn.close()
        return jsonify({'status': 'healthy', 'database': 'connected'})
    except Exception as e:
        return jsonify({'status': 'unhealthy', 'error': str(e)}), 500

if __name__ == '__main__':
    port = int(os.getenv('TRADING_API_PORT', '9200'))
    print(f"Starting Trading API server on port {port}")
    print(f"Database: {DB_CONFIG['host']}:{DB_CONFIG['port']}/{DB_CONFIG['database']}")
    app.run(host='0.0.0.0', port=port, debug=False)
