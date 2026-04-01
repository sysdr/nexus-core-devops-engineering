-- ============================================================
-- NexusCore Day 1: SQL → SurrealDB Migration Pattern Reference
-- Run these in SurrealDB Surrealist or via surreal sql CLI
-- ============================================================

-- SETUP: Start SurrealDB (in-memory for dev)
-- surreal start --log trace --user root --pass root memory
-- surreal sql --conn ws://localhost:8000 --user root --pass root --ns nexuscore --db tenants

-- ============================================================
-- PATTERN 1: Relational → Document + Relational hybrid
-- ============================================================

-- OLD SQL:
-- CREATE TABLE tenant (id SERIAL PRIMARY KEY, name VARCHAR(255), plan VARCHAR(50));
-- CREATE TABLE user (id SERIAL PRIMARY KEY, email VARCHAR(255), tenant_id INTEGER REFERENCES tenant(id));

-- SURREALDB: Define schema (optional — SurrealDB is schemaless by default)
DEFINE TABLE tenant SCHEMAFULL;
DEFINE FIELD name       ON tenant TYPE string;
DEFINE FIELD plan       ON tenant TYPE string  VALUE $value OR 'free';
DEFINE FIELD created_at ON tenant TYPE datetime DEFAULT time::now();

DEFINE TABLE user SCHEMAFULL;
DEFINE FIELD email     ON user TYPE string;
DEFINE FIELD tenant_id ON user TYPE record<tenant>;  -- typed record link (replaces FK)
DEFINE INDEX user_email ON user FIELDS email UNIQUE;

-- Insert (no surrogate keys needed — SurrealDB generates record IDs)
INSERT INTO tenant { id: tenant:acme, name: "Acme Corp", plan: "enterprise" };
INSERT INTO tenant { id: tenant:beta, name: "Beta Ltd",  plan: "free" };

INSERT INTO user { id: user:alice, email: "alice@acme.com", tenant_id: tenant:acme };
INSERT INTO user { id: user:bob,   email: "bob@beta.com",   tenant_id: tenant:beta };

-- OLD SQL SELECT with JOIN:
-- SELECT u.email, t.name FROM user u JOIN tenant t ON u.tenant_id = t.id WHERE t.plan = 'enterprise';

-- SURREALDB: Record links — no JOIN needed
SELECT email, tenant_id.name AS tenant_name
FROM user
WHERE tenant_id.plan = 'enterprise';

-- ============================================================
-- PATTERN 2: SQL many-to-many JOIN → Graph RELATE traversal
-- ============================================================

-- OLD SQL:
-- CREATE TABLE purchases (user_id INT, product_id INT, amount DECIMAL, PRIMARY KEY (user_id, product_id));
-- SELECT p.name, pu.amount FROM purchases pu JOIN product p ON pu.product_id = p.id WHERE pu.user_id = ?

-- SURREALDB: Graph edges are first-class records
DEFINE TABLE product SCHEMAFULL;
DEFINE FIELD name  ON product TYPE string;
DEFINE FIELD price ON product TYPE decimal;

INSERT INTO product { id: product:widget, name: "Widget Pro", price: 99.99 };
INSERT INTO product { id: product:gizmo,  name: "Gizmo X",    price: 149.00 };

-- RELATE creates a directed edge with optional payload
RELATE user:alice -> purchased -> product:widget CONTENT { amount: 1, at: time::now() };
RELATE user:alice -> purchased -> product:gizmo  CONTENT { amount: 2, at: time::now() };
RELATE user:bob   -> purchased -> product:widget CONTENT { amount: 1, at: time::now() };

-- Graph traversal — zero JOIN, O(degree) not O(N*M)
SELECT ->(purchased WHERE amount > 0)->product.* AS purchases
FROM user:alice;

-- Reverse traversal: who bought this product?
SELECT <-(purchased)<-user.email AS buyers
FROM product:widget;

-- Multi-hop: tenants that bought the same products as alice (graph pattern)
SELECT DISTINCT tenant_id.name AS co_tenant
FROM (SELECT <-(purchased)<-user AS buyers FROM (SELECT ->purchased->product FROM user:alice));

-- ============================================================
-- PATTERN 3: SQL LIKE → SurrealDB Full-Text BM25
-- ============================================================

-- OLD SQL: SELECT * FROM user WHERE email LIKE '%acme%'
-- Problem: LIKE requires full table scan, no ranking

-- SURREALDB: Define FTS analyzer (do once)
DEFINE ANALYZER IF NOT EXISTS email_analyzer
    TOKENIZERS class
    FILTERS ascii, lowercase;

DEFINE INDEX user_email_fts ON user
    FIELDS email
    SEARCH ANALYZER email_analyzer BM25;

-- Full-text search with relevance score
SELECT *, search::score(1) AS relevance
FROM user
WHERE email @1@ 'acme'
ORDER BY relevance DESC;

-- ============================================================
-- PATTERN 4: SQL triggers → SurrealDB LIVE SELECT (CDC)
-- ============================================================

-- OLD SQL approach: polling or custom trigger infrastructure
-- SURREALDB: LIVE SELECT — built-in CDC streaming

-- Start a live query (in SDK/WS connection):
LIVE SELECT * FROM user WHERE tenant_id = tenant:acme;
-- Returns a UUID. Subsequent CREATE/UPDATE/DELETE on matching records
-- emit change events to the WebSocket connection. Cancel with:
-- KILL <uuid>;

-- ============================================================
-- PATTERN 5: Transactions (unchanged semantics)
-- ============================================================

-- SurrealDB ACID transactions work as expected
BEGIN TRANSACTION;
  LET $t = (CREATE tenant:new_corp SET name = "New Corp", plan = "free");
  LET $u = (CREATE user SET email = "ceo@newcorp.com", tenant_id = $t.id);
  RELATE $u -> accessed -> (SELECT id FROM product LIMIT 1);
COMMIT TRANSACTION;

-- ============================================================
-- VERIFICATION QUERIES
-- ============================================================
SELECT count() AS total_tenants FROM tenant GROUP ALL;
SELECT count() AS total_users   FROM user   GROUP ALL;
SELECT count() AS total_edges   FROM purchased GROUP ALL;
SELECT graph::nodes(), graph::edges() FROM user:alice;
