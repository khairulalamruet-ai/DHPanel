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
  ModuleManagerPage,
  NodesPage,
  OperationsPage,
  SettingsPage,
  FeatureSetsPage,
  PlansPage,
  UsersPage,
  UserCreateWizardPage,
  UserDetailPage,
  ClientDashboardPage,
  ClientDomainsPage,
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
      <div className="flex h-screen w-screen items-center justify-center">
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
    <AppShell
      title={t("common.adminTitle")}
      navItems={navItems}
    >
      <Outlet />
    </AppShell>
  );
}

function RootLayout() {
  return (
    <QueryClientProvider client={queryClient}>
      <ThemeProvider defaultTheme="system">
        <ToastProvider>
          <SessionProvider>
            <AuthenticatedContent />
          </SessionProvider>
        </ToastProvider>
      </ThemeProvider>
    </QueryClientProvider>
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

// 7. Modules route (T03.12)
const modulesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/modules",
  component: ModuleManagerPage,
});

// 8. Nodes route (Cluster Nodes T02.16)
const nodesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/nodes",
  component: NodesPage,
});

// 9. Operations route (Operation Ledger T02.17)
const operationsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/operations",
  component: OperationsPage,
});

// 10. Settings route (T03.13)
const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: SettingsPage,
});

// 11. Feature Sets route (T04.02)
const featureSetsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/feature-sets",
  component: FeatureSetsPage,
});

// 12. Plans route (T04.04)
const plansRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/plans",
  component: PlansPage,
});

// 13. Users route (T04.15)
const usersRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/users",
  component: UsersPage,
});

// 14. User create wizard route (T04.16)
const userWizardRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/users/new",
  component: UserCreateWizardPage,
});

// 15. User detail route (T04.17)
const userDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/users/$id",
  component: UserDetailPage,
});

// 16. Client dashboard route (T04.18)
const clientRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/client",
  component: ClientDashboardPage,
});

// 17. Domains management route (T05.05)
const domainsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/domains",
  component: ClientDomainsPage,
});

// 18. Login route
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

// 19. Route Tree
const routeTree = rootRoute.addChildren([
  indexRoute,
  demoRoute,
  securityRoute,
  auditRoute,
  dummyRoute,
  modulesRoute,
  nodesRoute,
  operationsRoute,
  settingsRoute,
  featureSetsRoute,
  plansRoute,
  usersRoute,
  userWizardRoute,
  userDetailRoute,
  clientRoute,
  domainsRoute,
  loginRoute,
]);

// Create Router
export const router = createRouter({
  routeTree,
  defaultPreload: "intent",
  defaultErrorComponent: RouteErrorFallback,
  notFoundMode: "root",
});
