import * as fs from 'fs';
import * as path from 'path';
import * as http from 'http';
import EventSource from 'eventsource';
import { ConfigFile, ConfigEntry } from './types';

// Configuration will be loaded from config file

// Simple logger that writes to both console and file
class SimpleLogger {
  private fileStream: fs.WriteStream;
  
  constructor(filename: string, configName: string) {
    this.fileStream = fs.createWriteStream(filename, { flags: 'a' });
    this.log('info', `=== SSE Console Test Started at ${new Date().toISOString()} ===`);
    this.log('info', `Using configuration: ${configName}`);
  }
  
  log(level: string, message: string, data?: any) {
    const timestamp = new Date().toISOString();
    const logEntry = {
      timestamp,
      level,
      message,
      ...(data && { data })
    };
    
    // Console output - formatted for readability
    const color = level === 'error' ? '\x1b[31m' : level === 'info' ? '\x1b[36m' : '\x1b[0m';
    console.log(`${color}[${timestamp}] ${level.toUpperCase()}: ${message}\x1b[0m`);
    if (data) {
      console.log(JSON.stringify(data, null, 2));
    }
    
    // File output - JSON lines for parseability
    this.fileStream.write(JSON.stringify(logEntry) + '\n');
  }
  
  close() {
    this.log('info', '=== SSE Console Test Ended ===');
    this.fileStream.end();
  }
}

// Load configuration file
function loadConfig(configName: string): ConfigEntry {
  const configPath = path.join(__dirname, 'configs.json');
  
  if (!fs.existsSync(configPath)) {
    console.error(`Configuration file not found: ${configPath}`);
    process.exit(1);
  }
  
  try {
    const configData = fs.readFileSync(configPath, 'utf-8');
    const configs: ConfigFile = JSON.parse(configData);
    
    if (!configs[configName]) {
      console.error(`Configuration '${configName}' not found.`);
      console.log('\nAvailable configurations:');
      Object.keys(configs).forEach(key => {
        console.log(`  - ${key}: ${configs[key].description}`);
      });
      process.exit(1);
    }
    
    return configs[configName];
  } catch (error: any) {
    console.error(`Failed to load configuration: ${error.message}`);
    process.exit(1);
  }
}

// Parse command line arguments
function parseArgs(): string {
  const args = process.argv.slice(2);
  
  if (args.length === 0 || args[0] === '--help' || args[0] === '-h') {
    console.log('SSE Console Test Application');
    console.log('\nUsage: npm start <config-name>');
    console.log('       npm start --list');
    console.log('\nOptions:');
    console.log('  <config-name>  Name of the configuration to use');
    console.log('  --list         List available configurations');
    console.log('  --help, -h     Show this help message');
    console.log('\nExample:');
    console.log('  npm start price-ticker');
    console.log('  npm start portfolio');
    process.exit(0);
  }
  
  if (args[0] === '--list') {
    const configPath = path.join(__dirname, 'configs.json');
    if (fs.existsSync(configPath)) {
      const configData = fs.readFileSync(configPath, 'utf-8');
      const configs: ConfigFile = JSON.parse(configData);
      console.log('\nAvailable configurations:');
      Object.keys(configs).forEach(key => {
        console.log(`  - ${key}: ${configs[key].description}`);
      });
    } else {
      console.error('Configuration file not found');
    }
    process.exit(0);
  }
  
  return args[0];
}

// Get configuration name and load config
const configName = parseArgs();
const config = loadConfig(configName);

// Setup logger with config-specific filename
const LOG_FILE = `sse-events-${configName}-${new Date().toISOString().split('T')[0]}.log`;
const logger = new SimpleLogger(LOG_FILE, configName);

// Helper function to make API calls
function apiCall(method: string, endpoint: string, body?: any, serverUrl?: string): Promise<any> {
  return new Promise((resolve, reject) => {
    const url = new URL(`${serverUrl || config.server}${endpoint}`);
    logger.log('debug', `API Call: ${method} ${url}`, body);
    
    const options: http.RequestOptions = {
      hostname: url.hostname,
      port: url.port,
      path: url.pathname + url.search,
      method: method,
      headers: body ? { 'Content-Type': 'application/json' } : {}
    };
    
    const req = http.request(options, (res) => {
      let data = '';
      
      res.on('data', (chunk) => {
        data += chunk;
      });
      
      res.on('end', () => {
        let parsedData;
        try {
          parsedData = data ? JSON.parse(data) : null;
        } catch {
          parsedData = data;
        }
        
        logger.log('debug', `API Response: ${res.statusCode}`, parsedData);
        
        if (res.statusCode && res.statusCode >= 200 && res.statusCode < 300) {
          resolve(parsedData);
        } else {
          reject(new Error(`HTTP ${res.statusCode}: ${data}`));
        }
      });
    });
    
    req.on('error', (error) => {
      logger.log('error', `API Error: ${error.message}`);
      reject(error);
    });
    
    if (body) {
      req.write(JSON.stringify(body));
    }
    
    req.end();
  });
}

// Main application logic
async function main() {
  try {
    // Log configuration details
    logger.log('info', 'Configuration loaded:', {
      name: config.name,
      description: config.description,
      queryCount: config.queries.length,
      queryIds: config.queries.map(q => q.id),
      reactionId: config.reaction.id,
      ssePort: config.reaction.properties.port
    });
    
    // Check server health
    logger.log('info', `Checking Drasi Server health at ${config.server}...`);
    await apiCall('GET', '/health', undefined, config.server);
    logger.log('info', 'Drasi Server is healthy');
    
    // Create or verify queries from config (sequential)
    const createdQueryIds: string[] = [];
    const failedQueries: string[] = [];
    
    for (let i = 0; i < config.queries.length; i++) {
      const query = config.queries[i];
      logger.log('info', `Setting up query ${i + 1}/${config.queries.length}: ${query.id}...`);
      logger.log('debug', 'Query definition:', query);
      
      let retryCount = 0;
      const maxRetries = 2;
      let queryCreated = false;
      
      while (retryCount <= maxRetries && !queryCreated) {
        try {
          // Check if query already exists
          try {
            const existingQuery = await apiCall('GET', `/queries/${query.id}`, undefined, config.server);
            logger.log('info', `Query already exists: ${query.id}`, existingQuery.data);
            createdQueryIds.push(query.id);
            queryCreated = true;
            break;
          } catch {
            // Query doesn't exist, need to create it
          }
          
          // Create the query
          logger.log('info', `Creating new query: ${query.id}...`);
          await apiCall('POST', '/queries', query, config.server);
          logger.log('info', `Query created successfully: ${query.id}`);
          createdQueryIds.push(query.id);
          
          // Start the query if auto_start is true
          if (query.auto_start !== false) {
            await apiCall('POST', `/queries/${query.id}/start`, undefined, config.server);
            logger.log('info', `Query started: ${query.id}`);
          }
          
          queryCreated = true;
          
          // Small delay between query creations for stability
          if (i < config.queries.length - 1) {
            logger.log('debug', 'Waiting 1 second before next query...');
            await new Promise(resolve => setTimeout(resolve, 1000));
          }
        } catch (error: any) {
          retryCount++;
          if (retryCount <= maxRetries) {
            logger.log('warn', `Query creation failed, retrying (${retryCount}/${maxRetries}): ${error.message}`);
            await new Promise(resolve => setTimeout(resolve, 2000)); // Wait 2 seconds before retry
          } else {
            logger.log('error', `Failed to create query ${query.id} after ${maxRetries} retries: ${error.message}`);
            failedQueries.push(query.id);
            
            // Ask user if they want to continue with partial queries
            if (createdQueryIds.length > 0) {
              logger.log('warn', `Successfully created ${createdQueryIds.length} queries, failed on ${query.id}`);
              logger.log('warn', 'Continuing with available queries...');
              break; // Exit the while loop but continue to next query
            } else {
              // No queries created successfully, abort
              logger.log('error', 'No queries created successfully, aborting...');
              throw new Error(`Failed to create any queries. First failure: ${query.id}`);
            }
          }
        }
      }
    }
    
    // Report on query creation results
    if (failedQueries.length > 0) {
      logger.log('warn', `Query creation summary: ${createdQueryIds.length} succeeded, ${failedQueries.length} failed`);
      logger.log('warn', 'Failed queries:', failedQueries);
      
      if (createdQueryIds.length === 0) {
        logger.log('error', 'No queries available to create reaction');
        throw new Error('Cannot proceed without any queries');
      }
    } else {
      logger.log('info', `All ${createdQueryIds.length} queries set up successfully`);
    }
    
    logger.log('info', `All ${createdQueryIds.length} queries set up successfully`);
    
    // Create or verify SSE reaction from config
    logger.log('info', `Setting up SSE reaction: ${config.reaction.id}...`);
    logger.log('debug', 'Reaction definition:', config.reaction);
    
    // Ensure reaction is subscribed to all queries
    const reactionDef = {
      ...config.reaction,
      queries: createdQueryIds
    };
    
    try {
      const existingReaction = await apiCall('GET', `/reactions/${config.reaction.id}`, undefined, config.server);
      logger.log('info', 'Reaction already exists', existingReaction);
    } catch {
      logger.log('info', 'Creating new reaction...');
      await apiCall('POST', '/reactions', reactionDef, config.server);
      logger.log('info', 'Reaction created successfully');
      
      // Start the reaction if auto_start is true
      if (config.reaction.auto_start !== false) {
        await apiCall('POST', `/reactions/${config.reaction.id}/start`, undefined, config.server);
        logger.log('info', 'Reaction started');
      }
    }
    
    // Connect to SSE endpoint
    const ssePort = config.reaction.properties.port || 50051;
    const ssePath = config.reaction.properties.sse_path || '/events';
    const SSE_ENDPOINT = `http://localhost:${ssePort}${ssePath}`;
    
    logger.log('info', `Connecting to SSE endpoint: ${SSE_ENDPOINT}`);
    const eventSource = new EventSource(SSE_ENDPOINT);
    
    let eventCount = 0;
    const startTime = Date.now();
    
    eventSource.onopen = () => {
      logger.log('info', 'SSE connection established');
    };
    
    eventSource.onmessage = (event: MessageEvent) => {
      eventCount++;
      logger.log('event', `SSE Event #${eventCount}`, {
        id: event.lastEventId,
        type: event.type,
        data: event.data
      });
      
      // Try to parse and format the data
      try {
        const parsed = JSON.parse(event.data);
        logger.log('data', 'Parsed event data:', parsed);
        
        // If it looks like stock price data, format it nicely
        if (parsed.symbol && parsed.price !== undefined) {
          const changeColor = parsed.change_percent > 0 ? '\x1b[32m' : '\x1b[31m';
          console.log(`${changeColor}📈 ${parsed.symbol}: $${parsed.price} (${parsed.change_percent?.toFixed(2)}%)\x1b[0m`);
        }
      } catch {
        // Data wasn't JSON, just log as-is
      }
    };
    
    eventSource.onerror = (error: any) => {
      logger.log('error', 'SSE connection error', error);
      if (eventSource.readyState === EventSource.CLOSED) {
        logger.log('info', 'SSE connection closed, attempting to reconnect...');
      }
    };
    
    // Handle shutdown
    const shutdown = () => {
      const duration = Math.floor((Date.now() - startTime) / 1000);
      logger.log('info', `Shutting down... Received ${eventCount} events in ${duration} seconds`);
      eventSource.close();
      logger.close();
      process.exit(0);
    };
    
    process.on('SIGINT', shutdown);
    process.on('SIGTERM', shutdown);
    
    logger.log('info', 'Listening for SSE events... Press Ctrl+C to stop');
    
  } catch (error: any) {
    logger.log('error', `Fatal error: ${error.message}`, error);
    logger.close();
    process.exit(1);
  }
}

// Run the application
main().catch(error => {
  console.error('Unhandled error:', error);
  process.exit(1);
});