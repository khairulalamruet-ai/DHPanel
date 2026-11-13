export type FeatureCategoryKey =
  | "web"
  | "dns"
  | "db"
  | "files"
  | "mail"
  | "security";

export interface FeatureDefinition {
  key: string;
  title: { en: string; bn: string };
  description: { en: string; bn: string };
  category: FeatureCategoryKey;
  module: string;
}

export interface FeatureSet {
  id: string;
  name: string;
  description: string;
  features: string[];
  is_default: boolean;
  plans_count: number;
  created_at: string;
  updated_at: string;
}

export interface CreateFeatureSetPayload {
  name: string;
  description?: string;
  features: string[];
  is_default?: boolean;
}

export interface UpdateFeatureSetPayload {
  name?: string;
  description?: string;
  features?: string[];
  is_default?: boolean;
}
