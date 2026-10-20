-- NULL preserves legacy inheritance. [] explicitly means no header selection/library.
ALTER TABLE protocol_groups ADD COLUMN header_profiles_json TEXT;
ALTER TABLE protocols ADD COLUMN header_refs_json TEXT;
