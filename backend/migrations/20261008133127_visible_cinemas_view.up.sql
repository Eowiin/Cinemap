-- Add up migration script here
CREATE VIEW visible_cinemas AS
SELECT * FROM cinemas
WHERE lat IS NOT NULL
  AND lng IS NOT NULL
  AND updated_at >= datetime('now', '-14 days');
