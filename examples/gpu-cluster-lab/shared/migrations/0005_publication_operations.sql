-- The PostgreSQL source rejects publications that publish TRUNCATE, which it
-- cannot represent as row changes.
ALTER PUBLICATION gpu_demo_publication SET (publish = 'insert, update, delete');
