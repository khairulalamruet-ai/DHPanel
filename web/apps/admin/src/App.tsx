import React from "react";
import { ThemeProvider } from "@dhpanel/ui";
import { SessionProvider } from "@dhpanel/auth";
import { AppShell } from "@dhpanel/shell";
import { DashboardPage } from "@dhpanel/modules";
import { useTranslation } from "@dhpanel/i18n";

export function App() {
  const { t } = useTranslation();

  return (
    <ThemeProvider defaultTheme="system" storageKey="dhpanel-admin-theme">
      <SessionProvider bootstrapPath="/api/v1/admin/bootstrap">
        <AppShell title={t("common.adminTitle")}>
          <DashboardPage />
        </AppShell>
      </SessionProvider>
    </ThemeProvider>
  );
}
export default App;
