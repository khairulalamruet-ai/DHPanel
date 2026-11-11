import { http, HttpResponse } from "msw";

/**
 * MSW Mock Handlers
 * Missing endpoints are mocked according to docs/01-architecture and tagged with TODO(api:T<id>)
 */
export const handlers = [
  // TODO(api:T00.06): Backend Axum health check endpoint
  http.get(/\/api\/v1\/public\/health$/, () => {
    return HttpResponse.json({
      status: "ok",
      version: "0.1.0",
      timestamp: new Date().toISOString(),
    });
  }),

  // TODO(api:T01.01): Backend Admin bootstrap endpoint
  http.get(/\/api\/v1\/admin\/bootstrap$/, () => {
    return HttpResponse.json({
      user: {
        id: "usr_admin_01",
        username: "admin",
        email: "admin@dhpanel.local",
        name: "DHPanel Super Admin",
      },
      roles: ["super_admin"],
      permissions: [
        "dashboard.read",
        "users.read",
        "users.create",
        "domains.read",
        "domains.create",
        "settings.read",
      ],
      modules: ["dashboard", "users", "domains", "settings"],
      features: ["all"],
      limits: {},
      branding: {
        companyName: "DHPanel",
        logoUrl: "/logo.svg",
      },
      locale: "bn",
      csrfToken: "mock-csrf-token-admin-12345",
    });
  }),

  // TODO(api:T01.01): Backend Client bootstrap endpoint
  http.get(/\/api\/v1\/client\/bootstrap$/, () => {
    return HttpResponse.json({
      user: {
        id: "usr_client_01",
        username: "demo_user",
        email: "demo@example.com",
        name: "Demo Tenant User",
      },
      roles: ["client"],
      permissions: [
        "dashboard.read",
        "domains.read",
        "domains.create",
        "files.read",
      ],
      modules: ["dashboard", "domains", "files"],
      features: ["domains", "redirects"],
      limits: {
        max_domains: 5,
        used_domains: 1,
      },
      branding: {
        companyName: "DHPanel",
      },
      locale: "bn",
      csrfToken: "mock-csrf-token-client-12345",
    });
  }),
];
