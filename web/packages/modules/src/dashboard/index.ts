export * from "./types";
export * from "./HealthCard";
export * from "./DashboardWidgets";
export * from "./DashboardLayoutDialog";
export * from "./DashboardPage";

export const dashboardModule = {
  name: "dashboard",
  title: { en: "Dashboard", bn: "ড্যাশবোর্ড" },
  category: "core",
  core: true,
  permissions: ["dashboard.read"],
  features: ["dashboard"],
};
