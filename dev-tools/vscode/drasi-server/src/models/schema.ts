export interface ResourceKind {
  kind: string;
  title?: string;
  description?: string;
}

export interface PropertySchema {
  name: string;
  required: boolean;
  schema: Record<string, unknown>;
}

export interface ResourceSchema {
  kind: string;
  schema: Record<string, unknown>;
  properties: PropertySchema[];
  required: string[];
}
