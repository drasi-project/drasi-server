// Type definitions for SSE Console configuration

export interface QueryJoin {
  id: string;
  keys: Array<{
    label: string;
    property: string;
  }>;
}

export interface QueryDefinition {
  id: string;
  query: string;
  sources: string[];
  joins?: QueryJoin[];
  properties?: Record<string, any>;
  auto_start?: boolean;
}

export interface ReactionDefinition {
  id: string;
  reaction_type: string;
  properties: {
    host?: string;
    port?: number;
    sse_path?: string;
    heartbeat_interval_ms?: number;
  };
  auto_start?: boolean;
}

export interface ConfigEntry {
  name: string;
  description: string;
  server: string;  // Drasi Server URL
  queries: QueryDefinition[];  // Changed to array of queries
  reaction: ReactionDefinition;
}

export interface ConfigFile {
  [key: string]: ConfigEntry;
}