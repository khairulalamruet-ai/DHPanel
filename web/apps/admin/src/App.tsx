import React, { useState } from "react";
import { ThemeProvider, ToastProvider } from "@dhpanel/ui";
import { SessionProvider } from "@dhpanel/auth";
import { AppShell } from "@dhpanel/shell";
import { DashboardPage, UiDemoPage } from "@dhpanel/modules";
import { useTranslation } from "@dhpanel/i18n";
import { LayoutDashboard, Sparkles } from "lucide-react";

export function App() {
  const { t } = useTranslation();
  const [currentView, setCurrentView] = useState<"dashboard" | "demo">("dashboard");

  const customNav = [
    {
      id: "dashboard",
      labelKey: "common.dashboard",
      icon: <LayoutDashboard className="h-4 w-4" />,
      href: "#dashboard",
    },
    {
      id: "demo",
      labelKey: "UI Foundation (T00.12)",
      icon: <Sparkles className="h-4 w-4 text-primary" />,
      href: "#demo",
    },
  ];

  return (
    <ThemeProvider defaultTheme="system" storageKey="dhpanel-admin-theme">
      <ToastProvider>
        <SessionProvider bootstrapPath="/api/v1/admin/bootstrap">
          <AppShell
            title={t("common.adminTitle")}
            navItems={customNav.map((n) => ({
              ...n,
              href: "#",
            }))}
          >
            <div className="mb-6 flex gap-2 border-b pb-4">
              <button
                onClick={() => setCurrentView("dashboard")}
                className={`px-4 py-1.5 rounded-md text-sm font-medium transition-colors ${
                  currentView === "dashboard"
                    ? "bg-primary text-primary-foreground"
                    : "text-muted-foreground hover:bg-accent"
                }`}
              >
                {t("common.dashboard")}
              </button>
              <button
                onClick={() => setCurrentView("demo")}
                className={`px-4 py-1.5 rounded-md text-sm font-medium transition-colors ${
                  currentView === "demo"
                    ? "bg-primary text-primary-foreground"
                    : "text-muted-foreground hover:bg-accent"
                }`}
              >
                UI Components Demo (T00.12)
              </button>
            </div>

            {currentView === "dashboard" ? <DashboardPage /> : <UiDemoPage />}
          </AppShell>
        </SessionProvider>
      </ToastProvider>
    </ThemeProvider>
  );
}
export default App;
