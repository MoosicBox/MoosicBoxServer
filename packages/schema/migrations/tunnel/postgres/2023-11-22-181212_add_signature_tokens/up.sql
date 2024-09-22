CREATE TABLE IF NOT EXISTS signature_tokens (
    token_hash VARCHAR(64) PRIMARY KEY NOT NULL,
    client_id VARCHAR(64) NOT NULL,
    expires TIMESTAMP NOT NULL,
    created TIMESTAMP NOT NULL DEFAULT NOW(),
    updated TIMESTAMP NOT NULL DEFAULT NOW()
);