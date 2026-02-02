export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

export interface ComponentListItem {
  id: string;
  status: ComponentStatus;
  error_message?: string;
  links?: {
    self: string;
    full: string;
  };
  config?: Record<string, unknown>;
}

export interface InstanceListItem {
  id: string;
}

export type ComponentType = 'Source' | 'Query' | 'Reaction';

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
  componentType: ComponentType;
}

export interface Resource<TSpec = any> {
  kind: string;
  id: string;
  spec: TSpec;
  status?: Record<string, unknown>;
}
