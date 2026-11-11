import { http, HttpResponse } from "msw";

/**
 * MSW Mock Handlers
 * Real backend paths from repo-root openapi.json.
 */
let mfaEnabled = false;
const backupCodes = [
  "a1b2-c3d4",
  "e5f6-g7h8",
  "i9j0-k1l2",
  "m3n4-o5p6",
  "q7r8-s9t0",
  "u1v2-w3x4",
  "y5z6-a7b8",
  "c9d0-e1f2",
  "g3h4-i5j6",
  "k7l8-m9n0",
];

const getInitialSessions = () => [
  {
    id: "00000000-0000-0000-0000-000000000001",
    ip: "192.168.1.105",
    user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/130.0",
    created_at: new Date(Date.now() - 3600 * 1000 * 2).toISOString(),
    last_seen_at: new Date().toISOString(),
    expires_at: new Date(Date.now() + 86400 * 1000 * 7).toISOString(),
    current: true,
  },
  {
    id: "00000000-0000-0000-0000-000000000002",
    ip: "203.0.113.42",
    user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Firefox/131.0",
    created_at: new Date(Date.now() - 86400 * 1000 * 3).toISOString(),
    last_seen_at: new Date(Date.now() - 3600 * 1000 * 5).toISOString(),
    expires_at: new Date(Date.now() + 86400 * 1000 * 4).toISOString(),
    current: false,
  },
  {
    id: "00000000-0000-0000-0000-000000000003",
    ip: "198.51.100.12",
    user_agent: "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0) Mobile Safari/604.1",
    created_at: new Date(Date.now() - 86400 * 1000 * 7).toISOString(),
    last_seen_at: new Date(Date.now() - 86400 * 1000 * 2).toISOString(),
    expires_at: new Date(Date.now() + 86400 * 1000 * 1).toISOString(),
    current: false,
  },
];
let sessions = getInitialSessions();

const loginEvents = [
  {
    id: "evt_01",
    ip_address: "192.168.1.105",
    user_agent: "Chrome 130.0 (macOS)",
    status: "success" as const,
    created_at: new Date(Date.now() - 600 * 1000).toISOString(),
  },
  {
    id: "evt_02",
    ip_address: "192.168.1.105",
    user_agent: "Chrome 130.0 (macOS)",
    status: "2fa_prompt" as const,
    created_at: new Date(Date.now() - 650 * 1000).toISOString(),
  },
  {
    id: "evt_03",
    ip_address: "185.220.101.5",
    user_agent: "Python-urllib/3.10",
    status: "failed" as const,
    created_at: new Date(Date.now() - 3600 * 1000 * 4).toISOString(),
  },
  {
    id: "evt_04",
    ip_address: "203.0.113.42",
    user_agent: "Firefox 131.0 (Windows)",
    status: "success" as const,
    created_at: new Date(Date.now() - 86400 * 1000 * 3).toISOString(),
  },
];

const mockMe = {
  id: "00000000-0000-0000-0000-000000000001",
  username: "admin",
  email: "admin@dhpanel.local",
  kind: "admin",
  locale: "bn",
  mfa_enabled: false,
  permissions: ["*"],
  roles: ["super_admin"],
  csrf_token: "mock-csrf-token-admin-12345",
};

export const handlers = [
  // Health
  http.get(/\/api\/v1\/public\/health$/, () => {
    return HttpResponse.json({
      status: "ok",
      version: "0.1.0",
      timestamp: new Date().toISOString(),
    });
  }),

  // Auth: Current user (GET /api/v1/auth/me)
  http.get(/\/api\/v1\/auth\/me$/, () => {
    return HttpResponse.json({
      ...mockMe,
      mfa_enabled: mfaEnabled,
    });
  }),

  // Backward-compatibility: bootstrap endpoints
  http.get(/\/api\/v1\/admin\/bootstrap$/, () => {
    return HttpResponse.json({
      user: {
        id: mockMe.id,
        username: mockMe.username,
        email: mockMe.email,
        name: "DHPanel Super Admin",
      },
      roles: ["super_admin"],
      permissions: ["*"],
      modules: ["dashboard", "users", "domains", "settings", "security"],
      features: ["all"],
      limits: {},
      branding: {
        companyName: "DHPanel",
        logoUrl: "/logo.svg",
      },
      locale: "bn",
      csrfToken: mockMe.csrf_token,
    });
  }),

  http.get(/\/api\/v1\/client\/bootstrap$/, () => {
    return HttpResponse.json({
      user: {
        id: "00000000-0000-0000-0000-000000000002",
        username: "demo_user",
        email: "demo@example.com",
        name: "Demo Tenant User",
      },
      roles: ["client"],
      permissions: ["dashboard.read", "domains.read", "domains.create", "files.read"],
      modules: ["dashboard", "domains", "files", "security"],
      features: ["domains", "redirects"],
      limits: { max_domains: 5, used_domains: 1 },
      branding: { companyName: "DHPanel" },
      locale: "bn",
      csrfToken: "mock-csrf-token-client-12345",
    });
  }),

  // Auth: Login step 1 (POST /api/v1/public/auth/login)
  http.post(/\/api\/v1\/public\/auth\/login$/, async ({ request }) => {
    const body = (await request.json().catch(() => ({}))) as any;
    const login = body?.login || body?.username;
    const password = body?.password;

    if (!login || !password) {
      return HttpResponse.json(
        { title: "Invalid username or password", status: 401 },
        { status: 401 }
      );
    }

    if (login === "invalid" || password === "wrong") {
      return HttpResponse.json(
        { title: "Invalid username or password", status: 401 },
        { status: 401 }
      );
    }

    if (login === "admin_2fa") {
      return HttpResponse.json(
        {
          mfa_required: true,
          methods: ["totp"],
          expires_in: 300,
        },
        { status: 202 }
      );
    }

    return HttpResponse.json({
      ...mockMe,
      username: login,
      email: `${login}@dhpanel.local`,
    });
  }),

  // Auth: Login step 2 (POST /api/v1/public/auth/login/mfa)
  http.post(/\/api\/v1\/public\/auth\/login\/mfa$/, async ({ request }) => {
    const body = (await request.json().catch(() => ({}))) as any;
    const { code } = body || {};

    if (code !== "123456") {
      return HttpResponse.json(
        { title: "Invalid MFA code", status: 401 },
        { status: 401 }
      );
    }

    return HttpResponse.json({
      ...mockMe,
      username: "admin_2fa",
      email: "admin_2fa@dhpanel.local",
      mfa_enabled: true,
    });
  }),

  // Auth: Logout (POST /api/v1/auth/logout)
  http.post(/\/api\/v1\/auth\/logout$/, () => {
    return new HttpResponse(null, { status: 204 });
  }),

  // Auth: Sudo mode (POST /api/v1/auth/sudo)
  http.post(/\/api\/v1\/auth\/sudo$/, async ({ request }) => {
    const body = (await request.json().catch(() => ({}))) as any;
    if (body?.password === "wrong-password") {
      return HttpResponse.json(
        { title: "Incorrect password", status: 401 },
        { status: 401 }
      );
    }
    return new HttpResponse(null, { status: 204 });
  }),

  // MFA: Status (GET /api/v1/auth/mfa)
  http.get(/\/api\/v1\/auth\/mfa$/, () => {
    return HttpResponse.json({
      mfa_enabled: mfaEnabled,
      backup_codes_remaining: mfaEnabled ? backupCodes.length : 0,
    });
  }),

  // MFA: Enroll TOTP (POST /api/v1/auth/mfa/totp/enroll)
  http.post(/\/api\/v1\/auth\/mfa\/totp\/enroll$/, () => {
    return HttpResponse.json({
      secret: "JBSWY3DPEHPK3PXP",
      otpauth_uri: "otpauth://totp/DHPanel:admin@dhpanel.local?secret=JBSWY3DPEHPK3PXP&issuer=DHPanel",
    });
  }),

  // MFA: Confirm TOTP (POST /api/v1/auth/mfa/totp/confirm)
  http.post(/\/api\/v1\/auth\/mfa\/totp\/confirm$/, async ({ request }) => {
    const body = (await request.json().catch(() => ({}))) as any;
    const code = body?.code || body?.totp_code;

    if (code !== "123456") {
      return HttpResponse.json(
        { title: "Invalid 6-digit TOTP code", status: 400 },
        { status: 400 }
      );
    }

    mfaEnabled = true;
    return HttpResponse.json({
      backup_codes: backupCodes,
    });
  }),

  // MFA: Disable TOTP (POST /api/v1/auth/mfa/totp/disable)
  http.post(/\/api\/v1\/auth\/mfa\/totp\/disable$/, () => {
    mfaEnabled = false;
    return new HttpResponse(null, { status: 204 });
  }),

  // Sessions: List live sessions (GET /api/v1/auth/sessions)
  http.get(/\/api\/v1\/auth\/sessions$/, () => {
    if (sessions.length <= 1) {
      sessions = getInitialSessions();
    }
    return HttpResponse.json({ sessions });
  }),

  // Sessions: Revoke a session (DELETE /api/v1/auth/sessions/:id)
  http.delete(/\/api\/v1\/auth\/sessions\/(.+)$/, ({ params }) => {
    const id = params[0];
    sessions = sessions.filter((s) => s.id !== id);
    return new HttpResponse(null, { status: 204 });
  }),

  // Sessions: Revoke all other sessions (POST /api/v1/auth/sessions/revoke-others)
  http.post(/\/api\/v1\/auth\/sessions\/revoke-others$/, () => {
    const initialCount = sessions.length;
    sessions = sessions.filter((s) => s.current);
    return HttpResponse.json({
      revoked_count: Math.max(0, initialCount - sessions.length),
    });
  }),

  // Account Password change (POST /api/v1/account/password)
  http.post(/\/api\/v1\/account\/password$/, async ({ request }) => {
    const body = (await request.json().catch(() => ({}))) as any;
    const { current_password, new_password } = body || {};

    if (!current_password || !new_password) {
      return HttpResponse.json(
        { error: "bad_request", message: "Current and new password are required" },
        { status: 400 }
      );
    }

    if (current_password === "wrong-password") {
      return HttpResponse.json(
        { error: "invalid_current_password", message: "Current password does not match" },
        { status: 400 }
      );
    }

    if (new_password.length < 8) {
      return HttpResponse.json(
        { error: "weak_password", message: "Password must be at least 8 characters" },
        { status: 400 }
      );
    }

    return HttpResponse.json({ success: true, message: "Password updated successfully" });
  }),

  // Login History (GET /api/v1/account/login-history)
  http.get(/\/api\/v1\/account\/login-history$/, () => {
    return HttpResponse.json({ events: loginEvents });
  }),
];
