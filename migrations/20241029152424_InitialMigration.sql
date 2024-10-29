-- Add migration script here
CREATE TABLE IF NOT EXISTS messages (
  message_id INTEGER PRIMARY KEY NOT NULL,
  reply_to_id INTEGER NOT NULL,
  chat_id INTEGER NOT NULL,
  when_send DATETIME NOT NULL
);
