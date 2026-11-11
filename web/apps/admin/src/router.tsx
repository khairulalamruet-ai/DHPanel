import React from "react";
import {
  createRouter,
  createRoute,
  createRootRoute,
  Outlet,
} from "@tanstack/react-router";
import { ThemeProvider, ToastProvider } from "@dhpanel/ui";
import { SessionProvider, useSession, LoginPage, AccountSecurityPage } from "@dhpanel/auth";
import { queryClient, QueryClientProvider } from "@dhpanel/api";
import { AppShell, NotFoundPage, RouteErrorFallback } from "@dhpanel/shell";
import { DashboardPage, UiDemoPage, AuditViewerPage } from "@dhpanel/modules";
import { useTranslation } from "@dhpanel/i18n";
import { LayoutDashboard, Sparkles, ShieldCheck, FileText } from "lucide-react";

function AuthenticatedContent() {
  const { data: session, isLoading } = useSession();
  const { t } = useTranslation();

  const navItems = [
    {
      id: "dashboard",
      labelKey: "common.dashboard",
      icon: <LayoutDashboard className="h-4 w-4" />,
      href: "/",
    },
    {
      id: "security",
      labelKey: "security.nav",
      icon: <ShieldCheck className="h-4 w-4" />,
      href: "/security",
    },
    {
      id: "audit",
      labelKey: "audit.nav",
      icon: <FileText className="h-4 w-4" />,
      href: "/audit",
    },
    {
      id: "demo",
      labelKey: "UI Foundation (T00.12)",
      icon: <Sparkles className="h-4 w-4 text-primary" />,
      href: "/demo",
    },
  ];

  if (isLoading) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-background">
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      </div>
    );
  }

  if (!session?.user) {
    return (
      <LoginPage
        title={t("common.adminTitle")}
        onSuccess={() => router.navigate({ to: "/" })}
      />
    );
  }

  return (
    <AppShell title={t("common.adminTitle")} navItems={navItems}>
      <Outlet />
    </AppShell>
  );
}

// Root Component
function RootLayout() {
  return (
    <ThemeProvider defaultTheme="system" storageKey="dhpanel-admin-theme">
      <QueryClientProvider client={queryClient}>
        <ToastProvider>
          <SessionProvider bootstrapPath="/api/v1/auth/me">
            <AuthenticatedContent />
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

// 4. Security route
const securityRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/security",
  component: AccountSecurityPage,
});

// 5. Audit viewer route
const auditRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/audit",
  component: AuditViewerPage,
});

// 6. Login route
const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/login",
  component: () => {
    const { t } = useTranslation();
    return (
      <LoginPage
        title={t("common.adminTitle")}
        onSuccess={() => router.navigate({ to: "/" })}
      />
    );
  },
});

// 7. Route Tree
const routeTree = rootRoute.addChildren([indexRoute, demoRoute, securityRoute, auditRoute, loginRoute]);

// 6. Create Router
export const router = createRouter({
  routeTree,
  defaultPreload: "intent",
  defaultErrorComponent: RouteErrorFallback,
  notFoundMode: "root",
});
