# ০৫ — Frontend (React + Vite) গঠন

Next.js ব্যবহার হবে না। Vite দিয়ে একটা pnpm workspace হবে, যার দুটো entry: **admin** আর **client**। Reseller আলাদা app নয়, admin shell-এর ভেতরেই সীমিত permission নিয়ে চলবে।

```
web/
├── package.json  pnpm-workspace.yaml
├── apps/
│   ├── admin/                 # Vite app → dist/admin
│   │   ├── index.html
│   │   └── src/main.tsx       # admin shell, router, module registry (admin)
│   └── client/                # Vite app → dist/client
│       └── src/main.tsx
├── packages/
│   ├── ui/                    # shadcn/ui component, theme token, layout primitive
│   ├── api/                   # openapi থেকে তৈরি type + fetch client + query key
│   ├── auth/                  # session hook, permission ও feature guard
│   ├── i18n/                  # i18next setup, bn/en
│   ├── shell/                 # sidebar, topbar, breadcrumb, command palette
│   └── modules/               # ★ module অনুযায়ী feature
│       ├── domains/
│       │   ├── index.ts       # export { adminRoutes, clientRoutes, nav, featureKey }
│       │   ├── api.ts         # TanStack Query hook (useDomains, useAddDomain…)
│       │   ├── components/    # DomainTable, AddDomainDialog…
│       │   ├── pages/         # admin/…, client/…
│       │   ├── schemas.ts     # Zod schema (form)
│       │   └── locales/{bn,en}.json
│       ├── users/ plans/ dns/ ssl/ files/ …
└── e2e/                       # Playwright
```

## Shell-এর আচরণ
1. App চালু হলে `GET /bootstrap` → `{ user, roles, permissions[], modules[], features[], limits{}, branding, locale }`।
2. `module-registry` থেকে route ও nav তৈরি হয় → `modules[]`, `features[]` আর `permissions[]` দিয়ে ফিল্টার হয়।
3. `<Can permission="domains.create">` আর `<FeatureGate feature="redirects">` component থাকবে।
4. দীর্ঘ কাজের ক্ষেত্রে API `operation_id` ফেরত দেয় → `useOperation(id)` অবস্থা দেখায় (toast + progress)।
5. SSE `/events` → সংশ্লিষ্ট query invalidate হয় (যেমন `domains.changed` এলে domain তালিকা আবার আনা হয়)।

## UI নীতি
- Design token CSS variable-এ থাকবে। Light/dark theme। Branding (logo, রং) admin settings থেকে আসবে।
- Mobile-এও চলবে (responsive sidebar)।
- Accessibility: Radix primitive, keyboard navigation, focus ring, WCAG AA contrast।
- বাংলা font: Noto Sans Bengali / Hind Siliguri। সংখ্যা ও তারিখ locale অনুযায়ী দেখানো হবে।
- ঝুঁকিপূর্ণ কাজে (delete, suspend) confirm dialog থাকবে, যেখানে নাম টাইপ করে নিশ্চিত করতে হবে।
- Optimistic update শুধু নিরাপদ কাজে। Infra কাজে সবসময় operation অবস্থা দেখানো হবে।

## নিরাপত্তা
- Token `localStorage`-এ রাখা হবে না। শুধু HttpOnly session cookie।
- CSRF token: `/bootstrap` থেকে নিয়ে মেমরিতে রাখা হবে → `X-CSRF-Token` header-এ যাবে।
- CSP: `script-src 'self'`। Inline script থাকবে না। Admin custom code (OpenPanel-এর `custom.js`-এর মতো) এই কারণে শুধু client app-এ, sandboxed iframe-এ বা nonce দিয়ে চলবে, অথবা বাদ দেওয়া হবে।
