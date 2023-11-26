CREATE TABLE IF NOT EXISTS magic_tokens (
    magic_token VARCHAR(64) PRIMARY KEY NOT NULL,
    client_id VARCHAR(64) NOT NULL,
    access_token VARCHAR(64) NOT NULL,
    expires TEXT DEFAULT NULL,
    created TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now')),
    updated TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now'))
);
