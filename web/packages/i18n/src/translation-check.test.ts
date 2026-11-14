import { describe, it, expect } from "vitest";
import enJson from "./locales/en.json";
import bnJson from "./locales/bn.json";
import { validateTranslations, extractLeafKeys } from "./translation-check";

describe("Translation Integrity & Completeness (T18.08)", () => {
  describe("validateTranslations helper logic", () => {
    it("detects missing keys in Bengali translation", () => {
      const enMock = { common: { save: "Save", cancel: "Cancel" } };
      const bnMock = { common: { save: "সংরক্ষণ" } };

      const result = validateTranslations(enMock, bnMock);
      expect(result.valid).toBe(false);
      expect(result.missingInBn).toEqual(["common.cancel"]);
      expect(result.missingInEn).toEqual([]);
    });

    it("detects missing keys in English translation", () => {
      const enMock = { common: { save: "Save" } };
      const bnMock = { common: { save: "সংরক্ষণ", extra: "অতিরিক্ত" } };

      const result = validateTranslations(enMock, bnMock);
      expect(result.valid).toBe(false);
      expect(result.missingInBn).toEqual([]);
      expect(result.missingInEn).toEqual(["common.extra"]);
    });

    it("detects empty or blank translations", () => {
      const enMock = { common: { save: "   " } };
      const bnMock = { common: { save: "" } };

      const result = validateTranslations(enMock, bnMock);
      expect(result.valid).toBe(false);
      expect(result.emptyEnValues).toEqual(["common.save"]);
      expect(result.emptyBnValues).toEqual(["common.save"]);
    });

    it("passes when all nested keys match 1:1 with non-empty values", () => {
      const enMock = { auth: { title: "Login", button: { submit: "Submit" } } };
      const bnMock = { auth: { title: "লগইন", button: { submit: "জমা দিন" } } };

      const result = validateTranslations(enMock, bnMock);
      expect(result.valid).toBe(true);
      expect(result.missingInBn).toEqual([]);
      expect(result.missingInEn).toEqual([]);
    });
  });

  describe("Production locales/en.json vs locales/bn.json", () => {
    const result = validateTranslations(enJson, bnJson);

    it("ensures no keys are missing in bn.json", () => {
      if (result.missingInBn.length > 0) {
        // Output detailed failure for CI diagnostics
        console.error("Missing keys in bn.json:", result.missingInBn);
      }
      expect(
        result.missingInBn,
        `Found ${result.missingInBn.length} key(s) in en.json missing from bn.json`
      ).toEqual([]);
    });

    it("ensures no keys are missing in en.json", () => {
      if (result.missingInEn.length > 0) {
        console.error("Missing keys in en.json:", result.missingInEn);
      }
      expect(
        result.missingInEn,
        `Found ${result.missingInEn.length} key(s) in bn.json missing from en.json`
      ).toEqual([]);
    });

    it("ensures no keys have empty or whitespace-only values", () => {
      expect(
        result.emptyEnValues,
        `Found empty strings in en.json: ${result.emptyEnValues.join(", ")}`
      ).toEqual([]);
      expect(
        result.emptyBnValues,
        `Found empty strings in bn.json: ${result.emptyBnValues.join(", ")}`
      ).toEqual([]);
    });

    it("DoD: verification passes and key counts match across both languages", () => {
      expect(result.valid).toBe(true);
      expect(result.totalEnKeys).toBeGreaterThan(100);
      expect(result.totalEnKeys).toEqual(result.totalBnKeys);
    });
  });
});
