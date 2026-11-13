export type JsonSchemaType =
  | "string"
  | "integer"
  | "number"
  | "boolean"
  | "object"
  | "array";

export interface JsonSchemaProperty {
  type: JsonSchemaType;
  title?: string;
  description?: string;
  default?: any;
  format?: "email" | "password" | "textarea" | "uri" | "date-time";
  enum?: (string | number)[];
  enumNames?: string[];
  minimum?: number;
  maximum?: number;
  minLength?: number;
  maxLength?: number;
  readOnly?: boolean;
}

export interface JsonSchema {
  type: "object";
  title?: string;
  description?: string;
  required?: string[];
  properties: Record<string, JsonSchemaProperty>;
}

export interface SettingsModuleEntry {
  id: string;
  title: { en: string; bn: string };
  description: { en: string; bn: string };
  schema: JsonSchema;
  values: Record<string, any>;
}
