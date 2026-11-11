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

  // TODO(api:T01.05): Backend Login API with optional 2FA check
  http.post(/\/api\/v1\/public\/login$/, async ({ request }) => {
    const body = (await request.json().catch(() => ({}))) as any;
    const { username, password, totp_code } = body || {};

    if (!username || !password) {
      return HttpResponse.json(
        { error: "invalid_credentials", message: "Invalid username or password" },
        { status: 401 }
      );
    }

    if (username === "invalid" || password === "wrong") {
      return HttpResponse.json(
        { error: "invalid_credentials", message: "Invalid username or password" },
        { status: 401 }
      );
    }

    // If 2FA enabled account and code not yet provided, return 202
    if (username === "admin_2fa" && !totp_code) {
      return HttpResponse.json(
        { requires_2fa: true, message: "Please provide your 2FA verification code" },
        { status: 202 }
      );
    }

    if (username === "admin_2fa" && totp_code !== "123456") {
      return HttpResponse.json(
        { error: "invalid_2fa", message: "Invalid 2FA verification code" },
        { status: 401 }
      );
    }

    return HttpResponse.json({
      user: {
        id: "usr_admin_01",
        username: username || "admin",
        email: `${username || "admin"}@dhpanel.local`,
        name: "DHPanel Administrator",
      },
      csrf_token: "csrf-token-login-verified-9988",
    });
  }),

  // TODO(api:T01.05): Backend Logout API
  http.post(/\/api\/v1\/public\/logout$/, () => {
    return HttpResponse.json({ success: true });
  }),
];
