-- T05.18: per-domain HTTPS settings.
--
-- `force_https`: visitors who arrive over plain HTTP are sent to the same address
-- over HTTPS. `hsts`: answers over HTTPS carry a Strict-Transport-Security header
-- (one year), telling browsers to use HTTPS for this name from now on. HSTS
-- without forced HTTPS would lock visitors out of a site that still answers on
-- plain HTTP only in some places, so it needs `force_https`.
ALTER TABLE domains
    ADD COLUMN force_https boolean NOT NULL DEFAULT false,
    ADD COLUMN hsts boolean NOT NULL DEFAULT false,
    ADD CONSTRAINT domains_hsts_needs_https CHECK (NOT hsts OR force_https);
