CREATE TABLE IF NOT EXISTS scan_locations (
    `id` INTEGER PRIMARY KEY NOT NULL,
    `origin` VARCHAR(128) NOT NULL,
    `path` TEXT DEFAULT NULL,
    `created` TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now')),
    `updated` TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now'))
);
CREATE UNIQUE INDEX ux_scan_locations ON scan_locations(
    origin,
    ifnull(`path`, '')
);
