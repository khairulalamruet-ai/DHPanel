/**
 * Translation Completeness & Integrity Checker (T18.08)
 * Compares English (en.json) and Bengali (bn.json) key trees.
 */

export function extractLeafKeys(obj: Record<string, any>, prefix = ""): string[] {
  const keys: string[] = [];
  for (const [key, value] of Object.entries(obj)) {
    const fullKey = prefix ? `${prefix}.${key}` : key;
    if (value && typeof value === "object" && !Array.isArray(value)) {
      keys.push(...extractLeafKeys(value, fullKey));
    } else {
      keys.push(fullKey);
    }
  }
  return keys;
}

export interface TranslationValidationResult {
  valid: boolean;
  totalEnKeys: number;
  totalBnKeys: number;
  missingInBn: string[];
  missingInEn: string[];
  emptyEnValues: string[];
  emptyBnValues: string[];
}

export function validateTranslations(
  en: Record<string, any>,
  bn: Record<string, any>
): TranslationValidationResult {
  const enKeys = extractLeafKeys(en);
  const bnKeys = extractLeafKeys(bn);

  const enKeySet = new Set(enKeys);
  const bnKeySet = new Set(bnKeys);

  const missingInBn = enKeys.filter((k) => !bnKeySet.has(k));
  const missingInEn = bnKeys.filter((k) => !enKeySet.has(k));

  const getVal = (obj: any, path: string) =>
    path.split(".").reduce((acc, part) => (acc ? acc[part] : undefined), obj);

  const emptyEnValues = enKeys.filter((k) => {
    const val = getVal(en, k);
    return typeof val !== "string" || val.trim().length === 0;
  });

  const emptyBnValues = bnKeys.filter((k) => {
    const val = getVal(bn, k);
    return typeof val !== "string" || val.trim().length === 0;
  });

  return {
    valid:
      missingInBn.length === 0 &&
      missingInEn.length === 0 &&
      emptyEnValues.length === 0 &&
      emptyBnValues.length === 0,
    totalEnKeys: enKeys.length,
    totalBnKeys: bnKeys.length,
    missingInBn,
    missingInEn,
    emptyEnValues,
    emptyBnValues,
  };
}
