-- Queries and statements of a small shop.
CREATE TABLE IF NOT EXISTS customers (
    id INTEGER PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    "order" INTEGER DEFAULT 0,
    region TEXT REFERENCES regions (id)
);

INSERT INTO customers (id, name) VALUES (1, 'Ada'), (2, 'O''Brien');
INSERT INTO archive SELECT * FROM customers WHERE id < 10;
UPDATE customers SET name = 'Grace', "order" = "order" + 1 WHERE id = 1;
DELETE FROM customers WHERE name LIKE 'tmp%' OR id NOT IN (1, 2, 3);

/* Revenue per region,
   largest first. */
select c.region, count(*) as orders, sum(o.amount) total, avg(DISTINCT o.amount)
from customers c
     inner join orders o on o.customer = c.id
     left outer join regions r on r.id = c.region
     cross join (select 1 as one) as unit
where o.amount between 10 and 100 and not o.cancelled
  and c.id in (select customer from vip)
  and exists (select 1 from orders where amount > 1000)
group by c.region
having count(*) >= 2
order by total desc, c.region
limit 10 offset 20;

SELECT CASE WHEN amount < 0 THEN 'refund' WHEN amount = 0 THEN 'free' ELSE 'sale' END AS kind,
       -amount * 2 % 7 || 'x', (1 + 2) * 3, TRUE, NULL
FROM orders
