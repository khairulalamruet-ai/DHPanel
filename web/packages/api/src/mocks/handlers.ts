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

  // State for Account Security (T01.15)
  ...(() => {
    let mfaEnabled = false;
    let backupCodes = [
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
        id: "sess_curr_01",
        ip_address: "192.168.1.105",
        user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/130.0",
        created_at: new Date(Date.now() - 3600 * 1000 * 2).toISOString(),
        last_active_at: new Date().toISOString(),
        current: true,
      },
      {
        id: "sess_remote_02",
        ip_address: "203.0.113.42",
        user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Firefox/131.0",
        created_at: new Date(Date.now() - 86400 * 1000 * 3).toISOString(),
        last_active_at: new Date(Date.now() - 3600 * 1000 * 5).toISOString(),
        current: false,
      },
      {
        id: "sess_mobile_03",
        ip_address: "198.51.100.12",
        user_agent: "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0) Mobile Safari/604.1",
        created_at: new Date(Date.now() - 86400 * 1000 * 7).toISOString(),
        last_active_at: new Date(Date.now() - 86400 * 1000 * 2).toISOString(),
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

    return [
      // TODO(api:T01.02): Password change API
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

      // TODO(api:T01.08): 2FA Status
      http.get(/\/api\/v1\/account\/2fa\/status$/, () => {
        return HttpResponse.json({
          enabled: mfaEnabled,
          backup_codes_remaining: mfaEnabled ? backupCodes.length : 0,
        });
      }),

      // TODO(api:T01.08): 2FA Enroll
      http.post(/\/api\/v1\/account\/2fa\/enroll$/, () => {
        return HttpResponse.json({
          secret: "JBSWY3DPEHPK3PXP",
          qr_code_uri: "otpauth://totp/DHPanel:admin@dhpanel.local?secret=JBSWY3DPEHPK3PXP&issuer=DHPanel",
        });
      }),

      // TODO(api:T01.08): 2FA Confirm
      http.post(/\/api\/v1\/account\/2fa\/confirm$/, async ({ request }) => {
        const body = (await request.json().catch(() => ({}))) as any;
        const { totp_code } = body || {};

        if (totp_code !== "123456") {
          return HttpResponse.json(
            { error: "invalid_totp", message: "Invalid 6-digit TOTP code" },
            { status: 400 }
          );
        }

        mfaEnabled = true;
        return HttpResponse.json({
          success: true,
          backup_codes: backupCodes,
        });
      }),

      // TODO(api:T01.08): 2FA Disable
      http.post(/\/api\/v1\/account\/2fa\/disable$/, async ({ request }) => {
        const body = (await request.json().catch(() => ({}))) as any;
        const { password } = body || {};

        if (password === "wrong-password") {
          return HttpResponse.json(
            { error: "invalid_password", message: "Incorrect password" },
            { status: 400 }
          );
        }

        mfaEnabled = false;
        return HttpResponse.json({ success: true });
      }),

      // TODO(api:T01.08): 2FA Regenerate Backup Codes
      http.post(/\/api\/v1\/account\/2fa\/backup-codes$/, () => {
        backupCodes = Array.from({ length: 10 }, (_, i) => `new${i}-${Math.random().toString(36).slice(2, 6)}`);
        return HttpResponse.json({ backup_codes: backupCodes });
      }),

      // TODO(api:T01.10): List active sessions
      http.get(/\/api\/v1\/account\/sessions$/, () => {
        if (sessions.length <= 1) {
          sessions = getInitialSessions();
        }
        return HttpResponse.json({ sessions });
      }),

      // TODO(api:T01.10): Revoke a session
      http.delete(/\/api\/v1\/account\/sessions\/(.+)$/, ({ params }) => {
        const id = params[0];
        sessions = sessions.filter((s) => s.id !== id);
        return HttpResponse.json({ success: true });
      }),

      // TODO(api:T01.10): Revoke all other sessions
      http.post(/\/api\/v1\/account\/sessions\/revoke-others$/, () => {
        const initialCount = sessions.length;
        sessions = sessions.filter((s) => s.current);
        return HttpResponse.json({
          success: true,
          revoked_count: Math.max(0, initialCount - sessions.length),
        });
      }),

      // TODO(api:T01.01): Login history events
      http.get(/\/api\/v1\/account\/login-history$/, () => {
        return HttpResponse.json({ events: loginEvents });
      }),
    ];
  })(),
];
