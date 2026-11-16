-- Joins of every kind, and the negated and inequality forms of conditions.
SELECT a.x, b.y, c.z
FROM a
     RIGHT JOIN b ON a.id <> b.id
     FULL OUTER JOIN c ON c.id != a.id
     RIGHT OUTER JOIN d ON d.id = a.id
WHERE a.name NOT LIKE 'x%' AND a.n NOT BETWEEN 1 AND 2 AND a.m NOT IN (3, 4);
