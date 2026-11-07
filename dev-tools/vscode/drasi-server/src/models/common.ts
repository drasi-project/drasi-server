export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
  message?: string;
  code?: string;
}

export interface ComponentListItem {
  id: string;
  name: string;
  status: ComponentStatus;
  errorMessage?: string;
  links?: {
    self: string;
    full: string;
  };
  config?: Record<string, unknown>;
}

export interface InstanceListItem {
  id: string;
  sourceCount?: number;
  queryCount?: number;
  reactionCount?: number;
  links?: {
    self: string;
    sources: string;
    queries: string;
    reactions: string;
  };
}

export interface CreateInstanceRequest {
  id: string;
  allowExternalComponentIds?: boolean;
  persistIndex?: boolean;
  defaultPriorityQueueCapacity?: number;
  defaultDispatchBufferCapacity?: number;
}

export type ComponentType = 'Source' | 'Query' | 'Reaction';

export type ComponentLogType = ComponentType
  | 'BootstrapProvider' | 'IdentityProvider' | 'Transformer' | 'Sink' | 'Service';

export type ComponentStatus =
  | 'Stopped'
  | 'Starting'
  | 'Running'
  | 'Stopping'
  | 'Error'
  | 'Failed'
  | 'TerminalError'
  | 'Unknown';

export type LogLevel = 'Trace' | 'Debug' | 'Info' | 'Warn' | 'Error';

export interface ComponentEvent {
  componentId: string;
  componentType: ComponentType;
  status: ComponentStatus;
  timestamp: string;
  message?: string;
}

export interface LogMessage {
  timestamp: string;
  level: LogLevel;
  message: string;
  componentId: string;
  componentType: ComponentLogType;
}

export interface Resource<TSpec = any> {
  kind: string;
  name: string;
  id?: string;
  spec: TSpec;
  status?: Record<string, unknown>;
}

export interface ComponentIdentity {
  id: string;
  name: string;
}
