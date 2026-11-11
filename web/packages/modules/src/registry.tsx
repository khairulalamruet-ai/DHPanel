import React from "react";
import { LayoutDashboard, ShieldCheck, Sparkles, Puzzle, Globe, Users, Settings, FileText, Boxes, Server } from "lucide-react";

export interface ModuleManifest {
  name: string;
  title: { en: string; bn: string };
  category: string;
  core: boolean;
  permissions?: string[];
  features?: string[];
}

export interface ModuleNavItem {
  id: string;
  labelKey: string;
  icon: React.ReactNode;
  href: string;
  order?: number;
  permission?: string;
  feature?: string;
}

export interface FrontendModule {
  manifest: ModuleManifest;
  adminNav?: ModuleNavItem[];
  clientNav?: ModuleNavItem[];
}

export const registeredFrontendModules: FrontendModule[] = [
  {
    manifest: {
      name: "dashboard",
      title: { en: "Dashboard", bn: "ড্যাশবোর্ড" },
      category: "core",
      core: true,
      permissions: ["dashboard.read"],
      features: ["dashboard"],
    },
    adminNav: [
      {
        id: "dashboard",
        labelKey: "common.dashboard",
        icon: <LayoutDashboard className="h-4 w-4" />,
        href: "/",
        order: 10,
      },
    ],
    clientNav: [
      {
        id: "dashboard",
        labelKey: "common.dashboard",
        icon: <LayoutDashboard className="h-4 w-4" />,
        href: "/",
        order: 10,
      },
    ],
  },
  {
    manifest: {
      name: "security",
      title: { en: "Security", bn: "নিরাপত্তা" },
      category: "core",
      core: true,
      permissions: ["security.read"],
      features: ["security"],
    },
    adminNav: [
      {
        id: "security",
        labelKey: "security.nav",
        icon: <ShieldCheck className="h-4 w-4" />,
        href: "/security",
        order: 20,
      },
    ],
    clientNav: [
      {
        id: "security",
        labelKey: "security.nav",
        icon: <ShieldCheck className="h-4 w-4" />,
        href: "/security",
        order: 20,
      },
    ],
  },
  {
    manifest: {
      name: "nodes",
      title: { en: "Cluster Nodes", bn: "ক্লাস্টার নোডস" },
      category: "core",
      core: true,
      permissions: ["admin.nodes.read"],
    },
    adminNav: [
      {
        id: "nodes",
        labelKey: "nodes.nav",
        icon: <Server className="h-4 w-4" />,
        href: "/nodes",
        order: 22,
        permission: "admin.nodes.read",
      },
    ],
  },
  {
    manifest: {
      name: "audit",
      title: { en: "Audit Logs", bn: "অডিট লগ" },
      category: "core",
      core: true,
      permissions: ["admin.audit.read"],
    },
    adminNav: [
      {
        id: "audit",
        labelKey: "audit.nav",
        icon: <FileText className="h-4 w-4" />,
        href: "/audit",
        order: 25,
        permission: "admin.audit.read",
      },
    ],
  },
  {
    manifest: {
      name: "demo",
      title: { en: "UI Foundation", bn: "ইউআই ফাউন্ডেশন" },
      category: "core",
      core: true,
      features: ["ui_demo"],
    },
    adminNav: [
      {
        id: "demo",
        labelKey: "UI Foundation (T00.12)",
        icon: <Sparkles className="h-4 w-4 text-primary" />,
        href: "/demo",
        order: 90,
      },
    ],
  },
  {
    manifest: {
      name: "dummy",
      title: { en: "Dummy Module", bn: "ডামি মডিউল" },
      category: "plugins",
      core: false,
      permissions: ["dummy.read"],
      features: ["dummy_feature"],
    },
    adminNav: [
      {
        id: "dummy",
        labelKey: "dummy.nav",
        icon: <Puzzle className="h-4 w-4 text-amber-500" />,
        href: "/dummy",
        order: 50,
        permission: "dummy.read",
        feature: "dummy_feature",
      },
    ],
    clientNav: [
      {
        id: "dummy",
        labelKey: "dummy.nav",
        icon: <Puzzle className="h-4 w-4 text-amber-500" />,
        href: "/dummy",
        order: 50,
        permission: "dummy.read",
        feature: "dummy_feature",
      },
    ],
  },
  {
    manifest: {
      name: "domains",
      title: { en: "Domains", bn: "ডোমেইন" },
      category: "web",
      core: false,
      permissions: ["domains.read", "domains.create", "domains.update", "domains.delete"],
      features: ["domains", "redirects", "docroot"],
    },
    adminNav: [
      {
        id: "domains",
        labelKey: "common.domains",
        icon: <Globe className="h-4 w-4" />,
        href: "/domains",
        order: 30,
        permission: "domains.read",
        feature: "domains",
      },
    ],
  },
  {
    manifest: {
      name: "users",
      title: { en: "Users", bn: "ব্যবহারকারী" },
      category: "core",
      core: true,
      permissions: ["users.read", "users.create", "users.update", "users.delete"],
    },
    adminNav: [
      {
        id: "users",
        labelKey: "common.users",
        icon: <Users className="h-4 w-4" />,
        href: "/users",
        order: 40,
        permission: "users.read",
      },
    ],
  },
  {
    manifest: {
      name: "settings",
      title: { en: "Settings", bn: "সেটিংস" },
      category: "core",
      core: true,
      permissions: ["settings.read", "settings.update"],
    },
    adminNav: [
      {
        id: "settings",
        labelKey: "common.settings",
        icon: <Settings className="h-4 w-4" />,
        href: "/settings",
        order: 80,
        permission: "settings.read",
      },
    ],
  },
  {
    manifest: {
      name: "modules",
      title: { en: "Modules", bn: "মডিউলসমূহ" },
      category: "core",
      core: true,
      permissions: ["admin.modules.read"],
    },
    adminNav: [
      {
        id: "modules",
        labelKey: "modules.nav",
        icon: <Boxes className="h-4 w-4" />,
        href: "/modules",
        order: 70,
        permission: "admin.modules.read",
      },
    ],
  },
];

export const registeredModules: ModuleManifest[] = registeredFrontendModules.map((m) => m.manifest);

export interface FilterNavSession {
  roles?: string[];
  permissions?: string[];
  modules?: string[];
  features?: string[];
}

export function filterNavItems(
  modules: FrontendModule[],
  session: FilterNavSession | null | undefined,
  target: "admin" | "client" = "admin"
): ModuleNavItem[] {
  if (!session) return [];

  const activeModules = session.modules || [];
  const activeFeatures = session.features || [];
  const permissions = session.permissions || [];
  const roles = session.roles || [];

  const hasPerm = (perm: string | undefined): boolean => {
    if (!perm) return true;
    if (roles.includes("super_admin") || permissions.includes("*")) return true;
    if (permissions.includes(perm)) return true;
    return permissions.some((p) => {
      if (p.endsWith(".*")) {
        const prefix = p.slice(0, -2);
        return perm === prefix || perm.startsWith(`${prefix}.`);
      }
      return false;
    });
  };

  const hasFeat = (feat: string | undefined): boolean => {
    if (!feat) return true;
    if (activeFeatures.includes("all")) return true;
    return activeFeatures.includes(feat);
  };

  const navs: ModuleNavItem[] = [];

  for (const mod of modules) {
    // 1. Module Gate: Check if module is core or in active modules
    const isModuleEnabled = mod.manifest.core || activeModules.includes(mod.manifest.name);
    if (!isModuleEnabled) continue;

    // Pick target nav list
    const items = target === "admin" ? mod.adminNav : mod.clientNav;
    if (!items) continue;

    for (const item of items) {
      // 2. Feature Gate
      if (!hasFeat(item.feature)) continue;

      // 3. Permission Gate
      if (!hasPerm(item.permission)) continue;

      navs.push(item);
    }
  }

  // Sort by order
  return navs.sort((a, b) => (a.order || 100) - (b.order || 100));
}

export function getEnabledModules(activeModuleNames: string[]): ModuleManifest[] {
  return registeredModules.filter(
    (mod) => mod.core || activeModuleNames.includes(mod.name)
  );
}
