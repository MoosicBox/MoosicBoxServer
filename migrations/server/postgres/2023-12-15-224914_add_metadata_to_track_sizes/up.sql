ALTER TABLE track_sizes ADD COLUMN audio_bitrate BIGINT DEFAULT NULL;
ALTER TABLE track_sizes ADD COLUMN overall_bitrate BIGINT DEFAULT NULL;
ALTER TABLE track_sizes ADD COLUMN bit_depth BIGINT DEFAULT NULL;
ALTER TABLE track_sizes ADD COLUMN sample_rate BIGINT DEFAULT NULL;
ALTER TABLE track_sizes ADD COLUMN channels BIGINT DEFAULT NULL;