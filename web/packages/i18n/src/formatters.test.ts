import { describe, it, expect } from "vitest";
import {
  toBengaliDigits,
  formatNumber,
  formatCurrency,
  formatBytes,
  formatDate,
  formatRelativeTime,
} from "./formatters";

describe("i18n formatters", () => {
  describe("toBengaliDigits", () => {
    it("converts ASCII digits to Bengali digits", () => {
      expect(toBengaliDigits(1234567890)).toBe("১২৩৪৫৬৭৮৯০");
      expect(toBengaliDigits("Task 404")).toBe("Task ৪০৪");
    });
  });

  describe("formatNumber", () => {
    it("formats numbers in English locale", () => {
      const enNum = formatNumber(1250000, undefined, "en");
      expect(enNum).toBe("1,250,000");
    });

    it("formats numbers in Bengali locale", () => {
      const bnNum = formatNumber(1250000, undefined, "bn");
      expect(bnNum).toContain("১২");
      expect(bnNum).toContain("৫০");
      expect(bnNum).toContain("০০০");
    });
  });

  describe("formatCurrency", () => {
    it("formats BDT currency in English locale", () => {
      const res = formatCurrency(500, "BDT", "en");
      expect(res).toContain("500");
      expect(res).toMatch(/BDT|Tk/i);
    });

    it("formats BDT currency in Bengali locale", () => {
      const res = formatCurrency(500, "BDT", "bn");
      expect(res).toMatch(/৫০০/);
    });
  });

  describe("formatBytes", () => {
    it("formats zero bytes properly", () => {
      expect(formatBytes(0, "en")).toBe("0 Bytes");
      expect(formatBytes(0, "bn")).toBe("০ বাইট");
    });

    it("formats KB, MB, and GB in English", () => {
      expect(formatBytes(1024, "en")).toBe("1 KB");
      expect(formatBytes(1024 * 1024 * 2.5, "en")).toBe("2.5 MB");
      expect(formatBytes(1024 * 1024 * 1024 * 10, "en")).toBe("10 GB");
    });

    it("formats KB, MB, and GB in Bengali", () => {
      expect(formatBytes(1024, "bn")).toBe("১ কেবি");
      expect(formatBytes(1024 * 1024 * 2.5, "bn")).toBe("২.৫ এমবি");
      expect(formatBytes(1024 * 1024 * 1024 * 10, "bn")).toBe("১০ জিবি");
    });
  });

  describe("formatDate", () => {
    const testDate = new Date("2026-05-15T12:00:00Z");

    it("formats date in English locale", () => {
      const res = formatDate(testDate, undefined, "en");
      expect(res).toContain("May");
      expect(res).toContain("2026");
    });

    it("formats date in Bengali locale", () => {
      const res = formatDate(testDate, undefined, "bn");
      expect(res).toMatch(/[০-৯]|মে|২০২৬/);
    });
  });

  describe("formatRelativeTime", () => {
    it("formats past times", () => {
      const past = new Date(Date.now() - 1000 * 60 * 5); // 5 mins ago
      const enRes = formatRelativeTime(past, "en");
      expect(enRes).toContain("minute");

      const bnRes = formatRelativeTime(past, "bn");
      expect(bnRes).toMatch(/মিনিট|আগে/);
    });
  });
});
