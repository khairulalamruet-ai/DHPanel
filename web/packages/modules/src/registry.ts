import { dashboardModule } from "./dashboard";

export interface ModuleManifest {
  name: string;
  title: { en: string; bn: string };
  category: string;
  core: boolean;
  permissions?: string[];
  features?: string[];
}

export const registeredModules: ModuleManifest[] = [
  dashboardModule,
  {
    name: "domains",
    title: { en: "Domains", bn: "ডোমেইন" },
    category: "web",
    core: false,
    permissions: ["domains.read", "domains.create", "domains.update", "domains.delete"],
    features: ["domains", "redirects", "docroot"],
  },
  {
    name: "users",
    title: { en: "Users", bn: "ব্যবহারকারী" },
    category: "core",
    core: true,
    permissions: ["users.read", "users.create", "users.update", "users.delete"],
  },
  {
    name: "settings",
    title: { en: "Settings", bn: "সেটিংস" },
    category: "core",
    core: true,
    permissions: ["settings.read", "settings.update"],
  },
];

export function getEnabledModules(activeModuleNames: string[]): ModuleManifest[] {
  return registeredModules.filter(
    (mod) => mod.core || activeModuleNames.includes(mod.name)
  );
}
