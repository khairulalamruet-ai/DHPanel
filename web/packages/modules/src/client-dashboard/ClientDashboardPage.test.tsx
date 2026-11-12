import { describe, it, expect, vi, beforeEach } from "vitest";
import React from "react";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { ToastProvider } from "@dhpanel/ui";
import { ClientDashboardPage } from "./ClientDashboardPage";

function renderWithToast(ui: React.ReactElement) {
  return render(<ToastProvider>{ui}</ToastProvider>);
}

// Mock @dhpanel/i18n
vi.mock("@dhpanel/i18n", () => ({
  useTranslation: () => ({
    t: (key: string, opts?: any) => {
      const dict: Record<string, string> = {
        "common.error": "Error",
        "common.loading": "Loading...",
        "clientDashboard.welcome": `Welcome back, ${opts?.username || ""}!`,
        "clientDashboard.status.active": "Active",
        "clientDashboard.status.suspended": "Suspended",
        "clientDashboard.status.pending": "Pending",
        "clientDashboard.accountSecurity": "Account Security",
        "clientDashboard.viewWebsite": "View Website",
        "clientDashboard.resources.title": "Hardware & Quota Allocation",
        "clientDashboard.resources.desc": "Enforced cgroups and filesystem limits allocated to your hosting container",
        "clientDashboard.resources.disk": "Storage Quota",
        "clientDashboard.resources.bandwidth": "Monthly Traffic",
        "clientDashboard.resources.memory": "Memory (RAM)",
        "clientDashboard.resources.cpu": "CPU Cores",
        "clientDashboard.shortcuts.title": "Quick Tools & Shortcuts",
        "clientDashboard.shortcuts.desc": "Frequently used management tools and environment configurations",
        "clientDashboard.primaryDomain.title": "Primary Web Stack",
        "clientDashboard.primaryDomain.desc": "Web host details, web server document root, and SSL encryption status",
        "clientDashboard.activity.title": "Recent Stack Events",
        "clientDashboard.activity.desc": "Audit records and automated container health maintenance events",
      };
      return dict[key] || key;
    },
  }),
}));

const mockDashboardData = {
  tenant: {
    id: "usr_tenant_01",
    username: "client_alpha",
    email: "alpha@example.com",
    primary_domain: "alpha-hosting.com",
    plan_name: "Standard Business Plan",
    node_name: "node-sgp-01 (Singapore)",
    server_ip: "167.104.101.81",
    nameservers: ["ns1.dhpanel.net", "ns2.dhpanel.net"],
    created_at: "2026-10-01T00:00:00Z",
    status: "active" as const,
  },
  container: {
    status: "running" as const,
    name: "dh-tenant-alpha",
    image: "dhpanel/stack-php83-nginx:latest",
    php_version: "8.3.12",
    uptime: "Up 14 days, 6 hours",
    ip_address: "172.28.0.5",
  },
  resources: {
    disk_limit_mb: 20480,
    disk_used_mb: 1024,
    bandwidth_limit_mb: -1,
    bandwidth_used_mb: 4800,
    memory_limit_mb: 2048,
    memory_used_mb: 850,
    cpu_cores: 2,
    cpu_used_pct: 28,
    inodes_limit: 250000,
    inodes_used: 12450,
    databases_limit: 10,
    databases_used: 2,
    domains_limit: 5,
    domains_used: 2,
    emails_limit: 10,
    emails_used: 1,
  },
  primary_domain: {
    domain: "alpha-hosting.com",
    docroot: "/srv/tenants/client_alpha/domains/alpha-hosting.com/public_html",
    ssl_status: "active",
    ssl_issuer: "Let's Encrypt Authority X3",
    ssl_expires_in_days: 76,
    php_version: "PHP 8.3",
  },
  shortcuts: [
    { id: "files", title: "File Manager", icon: "Folder", href: "/files", phase: 5 },
    { id: "domains", title: "Domain Names", icon: "Globe", href: "/domains", phase: 6 },
    { id: "databases", title: "Databases", icon: "Database", href: "/databases", phase: 7 },
    { id: "ssl", title: "SSL / TLS", icon: "Lock", href: "/ssl", phase: 6 },
    { id: "cron", title: "Cron Jobs", icon: "Clock", href: "/cron", phase: 5 },
    { id: "php", title: "PHP Configuration", icon: "Code", href: "/php", phase: 6 },
    { id: "backups", title: "Backup & Restore", icon: "Archive", href: "/backups", phase: 8 },
    { id: "security", title: "Account Security", icon: "ShieldCheck", href: "/security", phase: 1 },
  ],
  recent_events: [
    {
      id: "ev_01",
      action: "user.login",
      timestamp: "2026-11-12T07:15:00Z",
      ip_address: "192.168.1.105",
      description: "Client portal session active",
    },
    {
      id: "ev_02",
      action: "ssl.renewed",
      timestamp: "2026-11-10T02:00:00Z",
      ip_address: "system",
      description: "SSL Certificate auto-renewed for alpha-hosting.com",
    },
  ],
};

const mockApiFetch = vi.fn();
vi.mock("@dhpanel/api", () => ({
  apiFetch: (...args: any[]) => mockApiFetch(...args),
}));

describe("ClientDashboardPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockApiFetch.mockResolvedValue(mockDashboardData);
  });

  it("renders tenant welcome card with username, plan, and container status", async () => {
    renderWithToast(<ClientDashboardPage />);

    await waitFor(() => {
      expect(
        screen.getByRole("heading", { level: 1, name: /Welcome back,\s*client_alpha!/i })
      ).toBeInTheDocument();
    });

    expect(screen.getAllByText("Active").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("Standard Business Plan")).toBeInTheDocument();
    expect(screen.getByText(/Stack:\s*running/i)).toBeInTheDocument();
    expect(screen.getByText(/PHP\s*8\.3\.12/i)).toBeInTheDocument();
    expect(screen.getByText("node-sgp-01 (Singapore)")).toBeInTheDocument();
    expect(screen.getByText("alpha@example.com")).toBeInTheDocument();
  });

  it("renders enforced resource quota cards with progress bars and numbers", async () => {
    renderWithToast(<ClientDashboardPage />);

    await waitFor(() => {
      expect(screen.getByText("Hardware & Quota Allocation")).toBeInTheDocument();
    });

    expect(screen.getByText("Storage Quota")).toBeInTheDocument();
    expect(screen.getByText("5%")).toBeInTheDocument();
    expect(screen.getByText("1.0 GB")).toBeInTheDocument();
    expect(screen.getByText("/ 20.0 GB")).toBeInTheDocument();

    expect(screen.getByText("Memory (RAM)")).toBeInTheDocument();
    expect(screen.getByText("42%")).toBeInTheDocument();
    expect(screen.getByText("850 MB")).toBeInTheDocument();
    expect(screen.getByText("/ 2.0 GB")).toBeInTheDocument();

    expect(screen.getByText("CPU Cores")).toBeInTheDocument();
    expect(screen.getByText("Load: 28%")).toBeInTheDocument();
    expect(screen.getByText("/ 2 Cores")).toBeInTheDocument();

    // Secondary metrics
    expect(screen.getByText("Inodes (Files)")).toBeInTheDocument();
    expect(screen.getByText("12,450 / 250,000")).toBeInTheDocument();
    expect(screen.getAllByText("Databases").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("2 / 10").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("2 / 5").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("Email Accounts")).toBeInTheDocument();
    expect(screen.getByText("1 / 10")).toBeInTheDocument();
  });

  it("renders shortcuts grid with available and upcoming module cards", async () => {
    renderWithToast(<ClientDashboardPage />);

    await waitFor(() => {
      expect(screen.getByText("Quick Tools & Shortcuts")).toBeInTheDocument();
    });

    expect(screen.getByText("File Manager")).toBeInTheDocument();
    expect(screen.getByText("Domain Names")).toBeInTheDocument();
    expect(screen.getAllByText("Databases").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("SSL / TLS")).toBeInTheDocument();
    expect(screen.getByText("Cron Jobs")).toBeInTheDocument();
    expect(screen.getByText("PHP Configuration")).toBeInTheDocument();
    expect(screen.getByText("Backup & Restore")).toBeInTheDocument();
    expect(screen.getAllByText("Account Security").length).toBeGreaterThanOrEqual(1);

    // Check phase badges
    expect(screen.getAllByText("Phase 05").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("Phase 06").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("Phase 07").length).toBeGreaterThanOrEqual(1);
  });

  it("renders primary web stack details and recent activity stream", async () => {
    renderWithToast(<ClientDashboardPage />);

    await waitFor(() => {
      expect(screen.getByText("Primary Web Stack")).toBeInTheDocument();
    });

    expect(screen.getAllByText("alpha-hosting.com").length).toBeGreaterThanOrEqual(1);
    expect(
      screen.getByText("/srv/tenants/client_alpha/domains/alpha-hosting.com/public_html")
    ).toBeInTheDocument();
    expect(
      screen.getByText("Let's Encrypt Authority X3 (76d remaining)")
    ).toBeInTheDocument();
    expect(screen.getByText("ns1.dhpanel.net, ns2.dhpanel.net")).toBeInTheDocument();

    // Activity
    expect(screen.getByText("Recent Stack Events")).toBeInTheDocument();
    expect(screen.getByText("user.login")).toBeInTheDocument();
    expect(screen.getByText("Client portal session active")).toBeInTheDocument();
  });

  it("handles upcoming phase shortcut clicks with friendly toast", async () => {
    renderWithToast(<ClientDashboardPage />);

    await waitFor(() => {
      expect(screen.getByText("File Manager")).toBeInTheDocument();
    });

    const fileCard = screen.getByText("File Manager");
    fireEvent.click(fileCard);

    await waitFor(() => {
      expect(screen.getByText("Scheduled for delivery in Phase 05")).toBeInTheDocument();
    });
  });
});
