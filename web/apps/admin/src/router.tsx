import React from "react";
import {
  createRouter,
  createRoute,
  createRootRoute,
  Outlet,
} from "@tanstack/react-router";
import { ThemeProvider, ToastProvider } from "@dhpanel/ui";
import { SessionProvider } from "@dhpanel/auth";
import { queryClient, QueryClientProvider } from "@dhpanel/api";
import { AppShell, NotFoundPage, RouteErrorFallback } from "@dhpanel/shell";
import { DashboardPage, UiDemoPage } from "@dhpanel/modules";
import { useTranslation } from "@dhpanel/i18n";
import { LayoutDashboard, Sparkles } from "lucide-react";

// Root Component
function RootLayout() {
  const { t } = useTranslation();

  const navItems = [
    {
      id: "dashboard",
      labelKey: "common.dashboard",
      icon: <LayoutDashboard className="h-4 w-4" />,
      href: "/",
    },
    {
      id: "demo",
      labelKey: "UI Foundation (T00.12)",
      icon: <Sparkles className="h-4 w-4 text-primary" />,
      href: "/demo",
    },
  ];

  return (
    <ThemeProvider defaultTheme="system" storageKey="dhpanel-admin-theme">
      <QueryClientProvider client={queryClient}>
        <ToastProvider>
          <SessionProvider bootstrapPath="/api/v1/admin/bootstrap">
            <AppShell title={t("common.adminTitle")} navItems={navItems}>
              <Outlet />
            </AppShell>
          </SessionProvider>
        </ToastProvider>
      </QueryClientProvider>
    </ThemeProvider>
  );
}

// 1. Root route
const rootRoute = createRootRoute({
  component: RootLayout,
  errorComponent: RouteErrorFallback,
  notFoundComponent: () => <NotFoundPage onNavigateHome={() => router.navigate({ to: "/" })} />,
});

// 2. Index route (Dashboard)
const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: DashboardPage,
});

// 3. Demo route (UI Demo)
const demoRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/demo",
  component: UiDemoPage,
});

// 4. Route Tree
const routeTree = rootRoute.addChildren([indexRoute, demoRoute]);

// 5. Create Router
export const router = createRouter({
  routeTree,
  defaultPreload: "intent",
  defaultErrorComponent: RouteErrorFallback,
  notFoundMode: "root",
});
