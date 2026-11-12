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
import { ClientDashboardPage, ClientDomainsPage, DnsEditorPage, SslPage, PhpManagementPage } from "@dhpanel/modules";
import { useTranslation } from "@dhpanel/i18n";
import { LayoutDashboard, ShieldCheck, Globe, Lock } from "lucide-react";

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
      id: "domains",
      labelKey: "domains.nav",
      icon: <Globe className="h-4 w-4" />,
      href: "/domains",
    },
    {
      id: "dns",
      labelKey: "dns.nav",
      icon: <Globe className="h-4 w-4" />,
      href: "/dns",
    },
    {
      id: "ssl",
      labelKey: "ssl.nav",
      icon: <Lock className="h-4 w-4" />,
      href: "/ssl",
    },
    {
      id: "security",
      labelKey: "security.nav",
      icon: <ShieldCheck className="h-4 w-4" />,
      href: "/security",
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
        title={t("common.clientTitle")}
        onSuccess={() => router.navigate({ to: "/" })}
      />
    );
  }

  return (
    <AppShell title={t("common.clientTitle")} navItems={navItems}>
      <Outlet />
    </AppShell>
  );
}

function RootLayout() {
  return (
    <ThemeProvider defaultTheme="system" storageKey="dhpanel-client-theme">
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

const rootRoute = createRootRoute({
  component: RootLayout,
  errorComponent: RouteErrorFallback,
  notFoundComponent: () => <NotFoundPage onNavigateHome={() => router.navigate({ to: "/" })} />,
});

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: ClientDashboardPage,
});

const domainsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/domains",
  component: ClientDomainsPage,
});

const dnsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/dns",
  component: DnsEditorPage,
});

const sslRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/ssl",
  component: SslPage,
});

const phpRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/php",
  component: PhpManagementPage,
});

const securityRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/security",
  component: AccountSecurityPage,
});

const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/login",
  component: () => {
    const { t } = useTranslation();
    return (
      <LoginPage
        title={t("common.clientTitle")}
        onSuccess={() => router.navigate({ to: "/" })}
      />
    );
  },
});

const routeTree = rootRoute.addChildren([indexRoute, domainsRoute, dnsRoute, sslRoute, phpRoute, securityRoute, loginRoute]);

export const router = createRouter({
  routeTree,
  defaultPreload: "intent",
  defaultErrorComponent: RouteErrorFallback,
  notFoundMode: "root",
});
