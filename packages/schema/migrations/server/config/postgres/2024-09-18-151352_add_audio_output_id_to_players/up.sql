DELETE FROM players;

ALTER TABLE players DROP COLUMN `type`;
ALTER TABLE players ADD COLUMN audio_output_id VARCHAR(128) NOT NULL;
