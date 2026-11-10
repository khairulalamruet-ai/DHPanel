import { describe, it, expect } from "vitest";
import { getEnabledModules } from "./registry";

describe("Module Registry", () => {
  it("includes core modules by default", () => {
    const enabled = getEnabledModules([]);
    const coreNames = enabled.map((m) => m.name);
    expect(coreNames).toContain("dashboard");
    expect(coreNames).toContain("users");
    expect(coreNames).toContain("settings");
    expect(coreNames).not.toContain("domains");
  });

  it("enables non-core modules when requested", () => {
    const enabled = getEnabledModules(["domains"]);
    const names = enabled.map((m) => m.name);
    expect(names).toContain("domains");
    expect(names).toContain("dashboard");
  });
});
