import psycopg2
conn = psycopg2.connect("postgres://noralpay:noralpay_dev_password@localhost:5433/paybank_monolith")
conn.autocommit = True
cur = conn.cursor()
cur.execute("DROP SCHEMA public CASCADE; CREATE SCHEMA public;")