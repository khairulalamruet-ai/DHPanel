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
  AdminDomainsPage,
  DnsEditorPage,
  SslPage,
  VhostTemplatesPage,
  PhpManagementPage,
  ServicesPage,
  ErrorPagesPage,
  ClientDatabasesPage,
  AdminDatabasesPage,
  FileManagerPage,
  DiskUsagePage,
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

// 17. Client domains route (T05.05)
const clientDomainsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/client/domains",
  component: ClientDomainsPage,
});

// 18. Domains management route (T05.06)
const domainsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/domains",
  component: AdminDomainsPage,
});

// 19. DNS Zone Editor route (T05.11)
const dnsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/dns",
  component: DnsEditorPage,
});

// 20. SSL / TLS Certificate route (T05.19)
const sslRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/ssl",
  component: SslPage,
});

// 21. VHost Template Management route (T06.07)
const vhostRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/vhost-templates",
  component: VhostTemplatesPage,
});

// 22. PHP Runtime Management route (T06.11)
const phpRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/php",
  component: PhpManagementPage,
});

// 23. Web Services Management route (T06.12)
const servicesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/services",
  component: ServicesPage,
});

// 24. Error & Suspended Pages route (T06.13)
const errorPagesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/error-pages",
  component: ErrorPagesPage,
});

// 25. Databases route (T07.10)
const clientDatabasesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/client/databases",
  component: ClientDatabasesPage,
});

const databasesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/databases",
  component: ClientDatabasesPage,
});

// 26. Admin Database Overview route (T07.11)
const adminDatabasesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/admin/databases",
  component: AdminDatabasesPage,
});

// 27. File Manager route (T08.09)
const clientFilesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/client/files",
  component: FileManagerPage,
});

const filesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/files",
  component: FileManagerPage,
});

// 28. Disk Usage route (T08.10)
const clientDiskUsageRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/client/disk-usage",
  component: DiskUsagePage,
});

const diskUsageRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/disk-usage",
  component: DiskUsagePage,
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
  clientDomainsRoute,
  domainsRoute,
  dnsRoute,
  sslRoute,
  vhostRoute,
  phpRoute,
  servicesRoute,
  errorPagesRoute,
  clientDatabasesRoute,
  databasesRoute,
  adminDatabasesRoute,
  clientFilesRoute,
  filesRoute,
  clientDiskUsageRoute,
  diskUsageRoute,
  loginRoute,
]);

// Create Router
export const router = createRouter({
  routeTree,
  defaultPreload: "intent",
  defaultErrorComponent: RouteErrorFallback,
  notFoundMode: "root",
});
