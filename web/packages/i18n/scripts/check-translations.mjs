#!/usr/bin/env node

/**
 * CLI Translation Completeness Checker (T18.08)
 * Checks that every translation key present in en.json exists in bn.json and vice versa.
 * Exits with status 1 if any key is missing.
 */

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const localesDir = path.resolve(__dirname, "../src/locales");

const enPath = path.join(localesDir, "en.json");
const bnPath = path.join(localesDir, "bn.json");

if (!fs.existsSync(enPath) || !fs.existsSync(bnPath)) {
  console.error("❌ Error: Locales files not found:", { enPath, bnPath });
  process.exit(1);
}

const en = JSON.parse(fs.readFileSync(enPath, "utf-8"));
const bn = JSON.parse(fs.readFileSync(bnPath, "utf-8"));

function extractLeafKeys(obj, prefix = "") {
  const keys = [];
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

const enKeys = extractLeafKeys(en);
const bnKeys = extractLeafKeys(bn);

const enKeySet = new Set(enKeys);
const bnKeySet = new Set(bnKeys);

const missingInBn = enKeys.filter((k) => !bnKeySet.has(k));
const missingInEn = bnKeys.filter((k) => !enKeySet.has(k));

console.log(`🌐 i18n Translation Integrity Check`);
console.log(`   - English (en.json) keys: ${enKeys.length}`);
console.log(`   - Bengali (bn.json) keys: ${bnKeys.length}`);

let hasErrors = false;

if (missingInBn.length > 0) {
  console.error(`\n❌ Missing in bn.json (${missingInBn.length} keys):`);
  missingInBn.forEach((k) => console.error(`   - ${k}`));
  hasErrors = true;
}

if (missingInEn.length > 0) {
  console.error(`\n❌ Missing in en.json (${missingInEn.length} keys):`);
  missingInEn.forEach((k) => console.error(`   - ${k}`));
  hasErrors = true;
}

if (hasErrors) {
  console.error(`\n❌ Translation check failed! All translation keys must match across languages.`);
  process.exit(1);
}

console.log(`\n✅ All ${enKeys.length} translation keys match 1:1 across en and bn. No missing keys.`);
process.exit(0);
