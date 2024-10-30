-- Adds the NOT NULL and AUTOINCREMENT specifiers to the message_id column
-- SQLite can't really change the type specs of columns, so we copy everything over to a new table
CREATE TABLE messages_temp
(
    message_id  INTEGER  NOT NULL PRIMARY KEY AUTOINCREMENT,
    reply_to_id INTEGER  NOT NULL,
    chat_id     INTEGER  NOT NULL,
    when_send   DATETIME NOT NULL
);

INSERT INTO messages_temp(message_id, reply_to_id, chat_id, when_send)
SELECT message_id, reply_to_id, chat_id, when_send
FROM messages;

DROP TABLE messages;

ALTER TABLE messages_temp
    RENAME TO messages;
