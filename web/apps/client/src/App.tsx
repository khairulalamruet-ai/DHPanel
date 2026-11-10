import React from "react";
import { ThemeProvider, ToastProvider } from "@dhpanel/ui";
import { SessionProvider } from "@dhpanel/auth";
import { AppShell } from "@dhpanel/shell";
import { DashboardPage } from "@dhpanel/modules";
import { useTranslation } from "@dhpanel/i18n";

export function App() {
  const { t } = useTranslation();

  return (
    <ThemeProvider defaultTheme="system" storageKey="dhpanel-client-theme">
      <ToastProvider>
        <SessionProvider bootstrapPath="/api/v1/client/bootstrap">
          <AppShell title={t("common.clientTitle")}>
            <DashboardPage />
          </AppShell>
        </SessionProvider>
      </ToastProvider>
    </ThemeProvider>
  );
}
export default App;
