ALTER TABLE artists DROP COLUMN qobuz_id;
ALTER TABLE albums DROP COLUMN qobuz_id;

DROP INDEX IF EXISTS ux_tracks_file;
CREATE UNIQUE INDEX ux_tracks_file ON tracks(
    ifnull(`file`, ''),
    `album_id`,
    `title`,
    `duration`,
    `number`,
    ifnull(`format`, ''),
    `source`,
    ifnull(`tidal_id`, 0)
);

ALTER TABLE tracks DROP COLUMN qobuz_id;
