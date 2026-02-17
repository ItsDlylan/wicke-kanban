ALTER TABLE tasks ADD COLUMN plan TEXT;
ALTER TABLE tasks ADD COLUMN plan_status TEXT DEFAULT NULL
    CHECK (plan_status IN ('pending', 'generating', 'completed', 'failed'));
