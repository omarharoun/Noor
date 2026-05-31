-- Least-privilege application DB role for Noor.
-- Run ONCE as the database owner/admin (e.g. neondb_owner). The running API then
-- connects as `noor_app` (DML only, no DDL), while schema migrations run as the
-- owner in a separate deploy step (set NOOR_SKIP_MIGRATE=true on the API).
--
-- This limits blast radius: a compromised API credential can read/write rows but
-- cannot DROP/ALTER tables or escalate.

-- 1. Create the role (use a strong, managed password; or use Neon's role UI).
CREATE ROLE noor_app LOGIN PASSWORD :'app_password';

-- 2. Connect + schema usage.
GRANT CONNECT ON DATABASE neondb TO noor_app;
GRANT USAGE ON SCHEMA public TO noor_app;

-- 3. DML on existing objects (NO DDL: no CREATE/DROP/ALTER/TRUNCATE).
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO noor_app;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO noor_app;

-- 4. Same grants for tables/sequences created by FUTURE migrations (run as owner).
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO noor_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT USAGE, SELECT ON SEQUENCES TO noor_app;

-- Usage:
--   psql "$OWNER_DATABASE_URL" -v app_password="'<strong-pw>'" -f least_privilege_role.sql
--   API:        DATABASE_URL=postgres://noor_app:...  NOOR_SKIP_MIGRATE=true
--   Migrations: DATABASE_URL=postgres://neondb_owner:...  sqlx migrate run
