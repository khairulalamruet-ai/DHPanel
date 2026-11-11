import { describe, it, expect, beforeEach } from "vitest";
import { api, setCsrfToken, getCsrfToken } from "./client";

describe("Typed OpenAPI Client & CSRF Interceptor", () => {
  beforeEach(() => {
    setCsrfToken(null);
  });

  it("calls health API and receives typed 200 response (DoD)", async () => {
    const { data, error, response } = await api.GET("/api/v1/public/health");

    expect(error).toBeUndefined();
    expect(response.status).toBe(200);
    expect(data).toBeDefined();
    expect(data?.status).toBe("ok");
    expect(data?.version).toBe("0.1.0");
  });

  it("tracks and manages CSRF token in memory", () => {
    expect(getCsrfToken()).toBeNull();
    setCsrfToken("csrf-token-abc-xyz");
    expect(getCsrfToken()).toBe("csrf-token-abc-xyz");
  });
});
