import psycopg2
import glob

conn = psycopg2.connect("postgres://noralpay:noralpay_dev_password@localhost:5433/paybank_monolith")
conn.autocommit = True
cur = conn.cursor()

for file in sorted(glob.glob("migrations/*.sql")):
    with open(file, 'r') as f:
        print(f"Running {file}")
        cur.execute(f.read())
print("Done")