export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

export interface ComponentListItem {
  id: string;
  status: ComponentStatus;
}

export interface InstanceListItem {
  id: string;
}

export type ComponentStatus =
  | 'Stopped'
  | 'Starting'
  | 'Running'
  | 'Stopping'
  | 'Failed'
  | 'TerminalError'
  | 'Unknown';

export interface Resource<TSpec = any> {
  kind: string;
  id: string;
  spec: TSpec;
  status?: Record<string, unknown>;
}
