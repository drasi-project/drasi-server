-- Copyright 2025 The Drasi Authors.
--
-- Licensed under the Apache License, Version 2.0 (the "License");
-- you may not use this file except in compliance with the License.
-- You may obtain a copy of the License at
--
--     http://www.apache.org/licenses/LICENSE-2.0
--
-- Unless required by applicable law or agreed to in writing, software
-- distributed under the License is distributed on an "AS IS" BASIS,
-- WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
-- See the License for the specific language governing permissions and
-- limitations under the License.

-- Stock Trading Demo Database Schema
-- This creates the necessary tables for the Drasi Server stock trading demo

-- Create user with replication privileges (if not exists)
-- Note: This should be run as the postgres superuser
DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_user WHERE usename = 'drasi_user') THEN
        CREATE USER drasi_user WITH REPLICATION LOGIN PASSWORD 'drasi_password';
    END IF;
END
$$;

-- Grant necessary permissions
GRANT CREATE ON DATABASE trading_demo TO drasi_user;
GRANT ALL PRIVILEGES ON DATABASE trading_demo TO drasi_user;

-- Drop existing tables if they exist
DROP TABLE IF EXISTS portfolio CASCADE;
DROP TABLE IF EXISTS stocks CASCADE;

-- Static stock information table
CREATE TABLE stocks (
    id SERIAL PRIMARY KEY,
    symbol VARCHAR(10) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    sector VARCHAR(100),
    industry VARCHAR(100),
    market_cap BIGINT,
    description TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Portfolio table for tracking user positions
CREATE TABLE portfolio (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(50) DEFAULT 'demo_user',
    symbol VARCHAR(10) NOT NULL,
    quantity INTEGER NOT NULL,
    purchase_price DECIMAL(10, 2) NOT NULL,
    purchase_date TIMESTAMP NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (symbol) REFERENCES stocks(symbol) ON DELETE CASCADE
);

-- Create indexes for performance
CREATE INDEX idx_stocks_symbol ON stocks(symbol);
CREATE INDEX idx_stocks_sector ON stocks(sector);
CREATE INDEX idx_portfolio_user_symbol ON portfolio(user_id, symbol);

-- Set REPLICA IDENTITY to FULL for CDC (Change Data Capture)
ALTER TABLE stocks REPLICA IDENTITY FULL;
ALTER TABLE portfolio REPLICA IDENTITY FULL;

-- Ensure drasi_user owns the tables
ALTER TABLE stocks OWNER TO drasi_user;
ALTER TABLE portfolio OWNER TO drasi_user;

-- Grant replication privileges to drasi_user
GRANT USAGE ON SCHEMA public TO drasi_user;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO drasi_user;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON TABLES TO drasi_user;

-- Create publication for logical replication
-- This defines which tables' changes will be streamed
CREATE PUBLICATION drasi_trading_pub FOR TABLE stocks, portfolio;

-- Create replication slot for Drasi Server
-- This maintains the position in the WAL stream
SELECT pg_create_logical_replication_slot('drasi_trading_slot', 'pgoutput');

-- Insert sample stock data
INSERT INTO stocks (symbol, name, sector, industry, market_cap) VALUES
    -- Technology stocks
    ('AAPL', 'Apple Inc.', 'Technology', 'Consumer Electronics', 3000000000000),
    ('MSFT', 'Microsoft Corporation', 'Technology', 'Software', 2800000000000),
    ('GOOGL', 'Alphabet Inc.', 'Technology', 'Internet Services', 1700000000000),
    ('META', 'Meta Platforms', 'Technology', 'Social Media', 900000000000),
    ('NVDA', 'NVIDIA Corporation', 'Technology', 'Semiconductors', 1100000000000),
    ('AMD', 'Advanced Micro Devices', 'Technology', 'Semiconductors', 230000000000),
    ('INTC', 'Intel Corporation', 'Technology', 'Semiconductors', 150000000000),
    ('CRM', 'Salesforce', 'Technology', 'Software', 270000000000),
    ('ORCL', 'Oracle Corporation', 'Technology', 'Software', 320000000000),
    ('ADBE', 'Adobe Inc.', 'Technology', 'Software', 220000000000),
    
    -- Financial stocks
    ('JPM', 'JPMorgan Chase', 'Financial', 'Banking', 500000000000),
    ('BAC', 'Bank of America', 'Financial', 'Banking', 280000000000),
    ('WFC', 'Wells Fargo', 'Financial', 'Banking', 200000000000),
    ('GS', 'Goldman Sachs', 'Financial', 'Investment Banking', 130000000000),
    ('MS', 'Morgan Stanley', 'Financial', 'Investment Banking', 150000000000),
    ('V', 'Visa Inc.', 'Financial', 'Payment Processing', 530000000000),
    ('MA', 'Mastercard', 'Financial', 'Payment Processing', 400000000000),
    ('AXP', 'American Express', 'Financial', 'Credit Services', 160000000000),
    ('BLK', 'BlackRock', 'Financial', 'Asset Management', 110000000000),
    ('SCHW', 'Charles Schwab', 'Financial', 'Brokerage', 130000000000),
    
    -- Healthcare stocks
    ('JNJ', 'Johnson & Johnson', 'Healthcare', 'Pharmaceuticals', 380000000000),
    ('UNH', 'UnitedHealth Group', 'Healthcare', 'Health Insurance', 520000000000),
    ('PFE', 'Pfizer Inc.', 'Healthcare', 'Pharmaceuticals', 160000000000),
    ('ABBV', 'AbbVie Inc.', 'Healthcare', 'Pharmaceuticals', 310000000000),
    ('MRK', 'Merck & Co.', 'Healthcare', 'Pharmaceuticals', 280000000000),
    ('LLY', 'Eli Lilly', 'Healthcare', 'Pharmaceuticals', 570000000000),
    ('CVS', 'CVS Health', 'Healthcare', 'Healthcare Services', 100000000000),
    ('MDT', 'Medtronic', 'Healthcare', 'Medical Devices', 85000000000),
    ('ABT', 'Abbott Laboratories', 'Healthcare', 'Medical Devices', 180000000000),
    ('TMO', 'Thermo Fisher Scientific', 'Healthcare', 'Life Sciences Tools', 230000000000),
    
    -- Consumer stocks
    ('AMZN', 'Amazon.com', 'Consumer', 'E-commerce', 1700000000000),
    ('TSLA', 'Tesla Inc.', 'Consumer', 'Electric Vehicles', 800000000000),
    ('WMT', 'Walmart Inc.', 'Consumer', 'Retail', 480000000000),
    ('HD', 'Home Depot', 'Consumer', 'Home Improvement', 400000000000),
    ('DIS', 'Walt Disney', 'Consumer', 'Entertainment', 170000000000),
    ('NKE', 'Nike Inc.', 'Consumer', 'Apparel', 150000000000),
    ('MCD', 'McDonalds', 'Consumer', 'Restaurants', 210000000000),
    ('SBUX', 'Starbucks', 'Consumer', 'Restaurants', 110000000000),
    ('PG', 'Procter & Gamble', 'Consumer', 'Consumer Goods', 380000000000),
    ('KO', 'Coca-Cola', 'Consumer', 'Beverages', 280000000000),
    
    -- Energy stocks
    ('XOM', 'Exxon Mobil', 'Energy', 'Oil & Gas', 450000000000),
    ('CVX', 'Chevron', 'Energy', 'Oil & Gas', 300000000000),
    ('COP', 'ConocoPhillips', 'Energy', 'Oil & Gas', 140000000000),
    ('SLB', 'Schlumberger', 'Energy', 'Oil Services', 70000000000),
    ('EOG', 'EOG Resources', 'Energy', 'Oil & Gas E&P', 75000000000),
    
    -- Industrial stocks
    ('BA', 'Boeing', 'Industrial', 'Aerospace', 130000000000),
    ('CAT', 'Caterpillar', 'Industrial', 'Machinery', 160000000000),
    ('GE', 'General Electric', 'Industrial', 'Conglomerate', 180000000000),
    ('UPS', 'United Parcel Service', 'Industrial', 'Logistics', 130000000000),
    ('HON', 'Honeywell', 'Industrial', 'Conglomerate', 140000000000);

-- Insert sample portfolio positions
INSERT INTO portfolio (user_id, symbol, quantity, purchase_price, purchase_date) VALUES
    ('demo_user', 'AAPL', 100, 165.00, '2024-01-15 10:30:00'),
    ('demo_user', 'MSFT', 50, 380.00, '2024-01-20 14:15:00'),
    ('demo_user', 'GOOGL', 75, 135.50, '2024-02-01 09:45:00'),
    ('demo_user', 'NVDA', 25, 750.00, '2024-02-10 11:20:00'),
    ('demo_user', 'TSLA', 40, 220.00, '2024-02-15 13:30:00'),
    ('demo_user', 'JPM', 80, 185.00, '2024-03-01 10:00:00'),
    ('demo_user', 'V', 60, 265.00, '2024-03-05 15:45:00'),
    ('demo_user', 'JNJ', 45, 150.00, '2024-03-10 12:00:00');

-- Create function to update timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Create trigger for stocks table
CREATE TRIGGER update_stocks_updated_at BEFORE UPDATE ON stocks
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Grant permissions to drasi_user
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO drasi_user;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO drasi_user;
GRANT USAGE ON SCHEMA public TO drasi_user;

-- Create replication slot for Drasi (if it doesn't exist)
-- Note: These must be created AFTER tables exist
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_replication_slots WHERE slot_name = 'drasi_trading_slot') THEN
        PERFORM pg_create_logical_replication_slot('drasi_trading_slot', 'pgoutput');
        RAISE NOTICE 'Created replication slot: drasi_trading_slot';
    ELSE
        RAISE NOTICE 'Replication slot drasi_trading_slot already exists';
    END IF;
END
$$;