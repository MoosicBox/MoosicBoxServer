CREATE TABLE IF NOT EXISTS connections (
    client_id VARCHAR(64) PRIMARY KEY NOT NULL,
    tunnel_ws_id VARCHAR(64) NOT NULL,
    created TIMESTAMP NOT NULL DEFAULT NOW(),
    updated TIMESTAMP NOT NULL DEFAULT NOW()
);
