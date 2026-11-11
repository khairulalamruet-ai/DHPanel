export type ModuleCategory =
  | "all"
  | "core"
  | "web"
  | "db"
  | "files"
  | "mail"
  | "security"
  | "system"
  | "apps";

export interface ModuleItem {
  name: string;
  title: { en: string; bn: string };
  description: { en: string; bn: string };
  version: string;
  category: "core" | "web" | "db" | "files" | "mail" | "security" | "system" | "apps";
  core: boolean;
  enabled: boolean;
  depends_on: string[];
  status: "active" | "disabled" | "error";
}

export interface ToggleModuleResponse {
  success: boolean;
  module: ModuleItem;
  disabled_dependents?: string[];
  error?: string;
  message?: string;
  dependents?: string[];
}
