-- Add migration script here
CREATE INDEX IF NOT EXISTS when_send_index ON messages (when_send);
