-- Task comments for discussing ideas before involving AI
CREATE TABLE task_comments (
    id BLOB PRIMARY KEY NOT NULL,
    task_id BLOB NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
);

CREATE INDEX idx_task_comments_task_id ON task_comments(task_id);

-- Junction table linking comments to images
CREATE TABLE comment_images (
    id BLOB PRIMARY KEY NOT NULL,
    comment_id BLOB NOT NULL,
    image_id BLOB NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY (comment_id) REFERENCES task_comments(id) ON DELETE CASCADE,
    FOREIGN KEY (image_id) REFERENCES images(id) ON DELETE CASCADE
);

CREATE INDEX idx_comment_images_comment_id ON comment_images(comment_id);
