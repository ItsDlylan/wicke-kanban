import { useCallback, useEffect, useRef, useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Alert } from '@/components/ui/alert';
import { taskCommentsApi, imagesApi } from '@/lib/api';
import type { TaskCommentWithImages, ImageResponse } from 'shared/types';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal } from '@/lib/modals';
import { ImagePlus, Send, Trash2, Pencil, X } from 'lucide-react';

export interface TaskCommentsDialogProps {
  taskId: string;
  taskTitle: string;
}

function CommentImageThumbnail({
  imageId,
  onRemove,
}: {
  imageId: string;
  onRemove?: () => void;
}) {
  return (
    <div className="relative group inline-block">
      <img
        src={`/api/images/${imageId}/file`}
        alt=""
        className="h-24 w-24 object-cover rounded border cursor-pointer"
        onClick={() => window.open(`/api/images/${imageId}/file`, '_blank')}
      />
      {onRemove && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          className="absolute -top-1.5 -right-1.5 bg-destructive text-destructive-foreground rounded-full p-0.5 opacity-0 group-hover:opacity-100 transition-opacity"
        >
          <X className="h-3 w-3" />
        </button>
      )}
    </div>
  );
}

function CommentItem({
  comment,
  onDelete,
  onUpdate,
}: {
  comment: TaskCommentWithImages;
  onDelete: (id: string) => void;
  onUpdate: (id: string, content: string, imageIds: string[]) => void;
}) {
  const [isEditing, setIsEditing] = useState(false);
  const [editContent, setEditContent] = useState(comment.content);
  const [editImageIds, setEditImageIds] = useState<string[]>(comment.image_ids);

  const handleSaveEdit = () => {
    if (!editContent.trim() && editImageIds.length === 0) return;
    onUpdate(comment.id, editContent, editImageIds);
    setIsEditing(false);
  };

  const handleCancelEdit = () => {
    setEditContent(comment.content);
    setEditImageIds(comment.image_ids);
    setIsEditing(false);
  };

  const date = new Date(comment.created_at);
  const timeStr = date.toLocaleString();

  if (isEditing) {
    return (
      <div className="border rounded-lg p-3 space-y-2 bg-muted/30">
        <Textarea
          value={editContent}
          onChange={(e) => setEditContent(e.target.value)}
          rows={3}
          autoFocus
        />
        {editImageIds.length > 0 && (
          <div className="flex flex-wrap gap-2">
            {editImageIds.map((imgId) => (
              <CommentImageThumbnail
                key={imgId}
                imageId={imgId}
                onRemove={() =>
                  setEditImageIds((ids) => ids.filter((id) => id !== imgId))
                }
              />
            ))}
          </div>
        )}
        <div className="flex gap-2 justify-end">
          <Button variant="ghost" size="sm" onClick={handleCancelEdit}>
            Cancel
          </Button>
          <Button size="sm" onClick={handleSaveEdit}>
            Save
          </Button>
        </div>
      </div>
    );
  }

  return (
    <div className="border rounded-lg p-3 space-y-2 group">
      <div className="flex items-start justify-between">
        <p className="text-sm whitespace-pre-wrap flex-1">{comment.content}</p>
        <div className="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity ml-2 shrink-0">
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={() => setIsEditing(true)}
          >
            <Pencil className="h-3.5 w-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 text-destructive"
            onClick={() => onDelete(comment.id)}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>
      {comment.image_ids.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {comment.image_ids.map((imgId) => (
            <CommentImageThumbnail key={imgId} imageId={imgId} />
          ))}
        </div>
      )}
      <p className="text-xs text-muted-foreground">{timeStr}</p>
    </div>
  );
}

const TaskCommentsDialogImpl = NiceModal.create<TaskCommentsDialogProps>(
  ({ taskId, taskTitle }) => {
    const modal = useModal();
    const [comments, setComments] = useState<TaskCommentWithImages[]>([]);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [newContent, setNewContent] = useState('');
    const [pendingImages, setPendingImages] = useState<ImageResponse[]>([]);
    const [isSending, setIsSending] = useState(false);
    const [isUploading, setIsUploading] = useState(false);
    const fileInputRef = useRef<HTMLInputElement>(null);
    const commentsEndRef = useRef<HTMLDivElement>(null);
    const textareaRef = useRef<HTMLTextAreaElement>(null);

    const loadComments = useCallback(async () => {
      try {
        setIsLoading(true);
        const data = await taskCommentsApi.list(taskId);
        setComments(data);
      } catch (err: unknown) {
        setError(
          err instanceof Error ? err.message : 'Failed to load comments'
        );
      } finally {
        setIsLoading(false);
      }
    }, [taskId]);

    useEffect(() => {
      if (modal.visible) {
        loadComments();
        setNewContent('');
        setPendingImages([]);
        setError(null);
      }
    }, [modal.visible, loadComments]);

    useEffect(() => {
      if (!isLoading) {
        commentsEndRef.current?.scrollIntoView({ behavior: 'smooth' });
      }
    }, [comments, isLoading]);

    const handleSend = async () => {
      if (!newContent.trim() && pendingImages.length === 0) return;

      setIsSending(true);
      setError(null);

      try {
        const created = await taskCommentsApi.create(taskId, {
          content: newContent,
          image_ids: pendingImages.map((img) => img.id),
        });
        setComments((prev) => [...prev, created]);
        setNewContent('');
        setPendingImages([]);
        textareaRef.current?.focus();
      } catch (err: unknown) {
        setError(err instanceof Error ? err.message : 'Failed to send comment');
      } finally {
        setIsSending(false);
      }
    };

    const handleDelete = async (commentId: string) => {
      try {
        await taskCommentsApi.delete(taskId, commentId);
        setComments((prev) => prev.filter((c) => c.id !== commentId));
      } catch (err: unknown) {
        setError(
          err instanceof Error ? err.message : 'Failed to delete comment'
        );
      }
    };

    const handleUpdate = async (
      commentId: string,
      content: string,
      imageIds: string[]
    ) => {
      try {
        const updated = await taskCommentsApi.update(taskId, commentId, {
          content,
          image_ids: imageIds,
        });
        setComments((prev) =>
          prev.map((c) => (c.id === commentId ? updated : c))
        );
      } catch (err: unknown) {
        setError(
          err instanceof Error ? err.message : 'Failed to update comment'
        );
      }
    };

    const handleImageUpload = async (files: FileList | File[]) => {
      setIsUploading(true);
      setError(null);

      try {
        for (const file of Array.from(files)) {
          if (!file.type.startsWith('image/')) continue;
          const img = await imagesApi.upload(file);
          setPendingImages((prev) => [...prev, img]);
        }
      } catch (err: unknown) {
        setError(err instanceof Error ? err.message : 'Failed to upload image');
      } finally {
        setIsUploading(false);
      }
    };

    const handlePaste = (e: React.ClipboardEvent) => {
      const items = e.clipboardData?.items;
      if (!items) return;

      const imageFiles: File[] = [];
      for (const item of Array.from(items)) {
        if (item.type.startsWith('image/')) {
          const file = item.getAsFile();
          if (file) imageFiles.push(file);
        }
      }

      if (imageFiles.length > 0) {
        e.preventDefault();
        handleImageUpload(imageFiles);
      }
    };

    const handleDrop = (e: React.DragEvent) => {
      e.preventDefault();
      if (e.dataTransfer.files.length > 0) {
        handleImageUpload(e.dataTransfer.files);
      }
    };

    const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        handleSend();
      }
    };

    const handleClose = () => {
      modal.resolve();
      modal.hide();
    };

    return (
      <Dialog
        open={modal.visible}
        onOpenChange={(open) => !open && handleClose()}
      >
        <DialogContent className="max-w-2xl max-h-[80vh] flex flex-col">
          <DialogHeader>
            <DialogTitle>Comments</DialogTitle>
            <DialogDescription>{taskTitle}</DialogDescription>
          </DialogHeader>

          <div className="flex-1 overflow-y-auto space-y-3 min-h-[200px] max-h-[400px] pr-1">
            {isLoading ? (
              <div className="py-8 text-center text-muted-foreground">
                Loading comments...
              </div>
            ) : comments.length === 0 ? (
              <div className="py-8 text-center text-muted-foreground">
                No comments yet. Start discussing your idea below.
              </div>
            ) : (
              comments.map((comment) => (
                <CommentItem
                  key={comment.id}
                  comment={comment}
                  onDelete={handleDelete}
                  onUpdate={handleUpdate}
                />
              ))
            )}
            <div ref={commentsEndRef} />
          </div>

          {error && (
            <Alert variant="destructive" className="mt-2">
              {error}
            </Alert>
          )}

          <div
            className="border rounded-lg p-3 space-y-2 mt-2"
            onDrop={handleDrop}
            onDragOver={(e) => e.preventDefault()}
          >
            {pendingImages.length > 0 && (
              <div className="flex flex-wrap gap-2">
                {pendingImages.map((img) => (
                  <CommentImageThumbnail
                    key={img.id}
                    imageId={img.id}
                    onRemove={() =>
                      setPendingImages((prev) =>
                        prev.filter((i) => i.id !== img.id)
                      )
                    }
                  />
                ))}
              </div>
            )}
            <div className="flex gap-2">
              <Textarea
                ref={textareaRef}
                value={newContent}
                onChange={(e) => setNewContent(e.target.value)}
                onPaste={handlePaste}
                onKeyDown={handleKeyDown}
                placeholder="Write a comment... (paste images, or Cmd+Enter to send)"
                rows={2}
                className="flex-1 resize-none"
                disabled={isSending}
              />
              <div className="flex flex-col gap-1">
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8"
                  onClick={() => fileInputRef.current?.click()}
                  disabled={isUploading}
                >
                  <ImagePlus className="h-4 w-4" />
                </Button>
                <Button
                  size="icon"
                  className="h-8 w-8"
                  onClick={handleSend}
                  disabled={
                    isSending ||
                    (!newContent.trim() && pendingImages.length === 0)
                  }
                >
                  <Send className="h-4 w-4" />
                </Button>
              </div>
            </div>
            <input
              ref={fileInputRef}
              type="file"
              accept="image/*"
              multiple
              className="hidden"
              onChange={(e) => {
                if (e.target.files) handleImageUpload(e.target.files);
                e.target.value = '';
              }}
            />
          </div>
        </DialogContent>
      </Dialog>
    );
  }
);

export const TaskCommentsDialog = defineModal<TaskCommentsDialogProps, void>(
  TaskCommentsDialogImpl
);
