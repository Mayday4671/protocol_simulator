-- Before-send log watermark distinguishes early replies from pre-request traffic.
ALTER TABLE logs ADD COLUMN send_after_log_id INTEGER;
