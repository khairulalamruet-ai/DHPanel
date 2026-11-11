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
import {
  DashboardPage,
  UiDemoPage,
  AuditViewerPage,
  DummyModulePage,
  filterNavItems,
  registeredFrontendModules,
} from "@dhpanel/modules";
import { useTranslation } from "@dhpanel/i18n";

function AuthenticatedContent() {
  const { data: session, isLoading } = useSession();
  const { t } = useTranslation();

  const navItems = filterNavItems(registeredFrontendModules, session, "admin");

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

// 6. Dummy module route (T03.11 DoD)
const dummyRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/dummy",
  component: DummyModulePage,
});

// 7. Login route
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

// 8. Route Tree
const routeTree = rootRoute.addChildren([
  indexRoute,
  demoRoute,
  securityRoute,
  auditRoute,
  dummyRoute,
  loginRoute,
]);

// 6. Create Router
export const router = createRouter({
  routeTree,
  defaultPreload: "intent",
  defaultErrorComponent: RouteErrorFallback,
  notFoundMode: "root",
});
