/**
 * Production `/shelves/:id` page.
 *
 * Renders the shelf's items in stored order with a drag-to-reorder
 * affordance backed by `@dnd-kit/sortable`. Reorder uses an
 * optimistic react-query mutation: the new order is committed to the
 * cache before the network round-trip, and rolled back on 412
 * (If-Match precondition failure) with a toast prompting the operator
 * to refresh.
 */
import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  arrayMove,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { GripVertical } from "lucide-react";
import { Suspense, type ReactElement } from "react";
import { useParams } from "react-router";
import { toast } from "sonner";

import { ApiError, getShelf, reorderShelfItems, type ShelfWithItems } from "@/api";
import { Skeleton } from "@/components/ui/skeleton";
import { queryKeys } from "@/lib/query/keys";

/** Top-level page component (Suspense boundary + content). */
export function ShelfDetailPage(): ReactElement {
  return (
    <Suspense fallback={<ShelfDetailSkeleton />}>
      <ShelfDetailContent />
    </Suspense>
  );
}

function ShelfDetailContent(): ReactElement {
  const qc = useQueryClient();
  const params = useParams<{ id: string }>();
  const id = params.id ?? "";
  const queryKey = queryKeys.shelves.detail(id);
  const { data } = useSuspenseQuery<ShelfWithItems>({
    queryKey,
    queryFn: ({ signal }) => getShelf(id, signal),
  });

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  const reorder = useMutation({
    mutationFn: ({ items, ifMatch }: { items: string[]; ifMatch: string }) =>
      reorderShelfItems(id, items, ifMatch),
    onMutate: async ({ items }) => {
      await qc.cancelQueries({ queryKey });
      const previous = qc.getQueryData<ShelfWithItems>(queryKey);
      if (previous !== undefined) {
        const reordered: ShelfWithItems = {
          ...previous,
          items: items
            .map((mid, idx) => {
              const original = previous.items.find((it) => it.manifestation_id === mid);
              if (original === undefined) return null;
              return { ...original, position: idx };
            })
            .filter((it): it is NonNullable<typeof it> => it !== null),
        };
        qc.setQueryData(queryKey, reordered);
      }
      return { previous };
    },
    onError: (err, _vars, ctx) => {
      if (ctx?.previous !== undefined) qc.setQueryData(queryKey, ctx.previous);
      if (err instanceof ApiError && err.status === 412) {
        toast.error("Shelf changed on another device — refresh and retry.");
      } else {
        toast.error(err instanceof ApiError ? err.detail : "Reorder failed.");
      }
      // No invalidate here — `onSettled` handles the refetch on both
      // success and failure paths. Doing both burns a duplicate
      // round-trip and can flash stale → reverted → fresh.
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey });
    },
  });

  function handleDragEnd(event: DragEndEvent): void {
    const { active, over } = event;
    if (over === null || active.id === over.id) return;
    const ids = data.items.map((it) => it.manifestation_id);
    const oldIndex = ids.indexOf(String(active.id));
    const newIndex = ids.indexOf(String(over.id));
    if (oldIndex < 0 || newIndex < 0) return;
    const next = arrayMove(ids, oldIndex, newIndex);
    reorder.mutate({ items: next, ifMatch: data.updated_at });
  }

  return (
    <div className="mx-auto max-w-3xl px-6 py-10 sm:px-10">
      <header className="mb-6">
        <p className="text-fg-muted text-xs uppercase tracking-wider">Shelf</p>
        <h1 className="font-display mt-1 text-3xl font-semibold tracking-tight text-fg">
          {data.name}
        </h1>
        <p className="text-fg-muted mt-2 text-sm">
          {data.items.length} {data.items.length === 1 ? "item" : "items"}
        </p>
      </header>
      <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
        <SortableContext
          items={data.items.map((it) => it.manifestation_id)}
          strategy={verticalListSortingStrategy}
        >
          <ul className="space-y-2">
            {data.items.map((item) => (
              <SortableItem key={item.manifestation_id} id={item.manifestation_id} />
            ))}
          </ul>
        </SortableContext>
      </DndContext>
    </div>
  );
}

interface SortableItemProps {
  id: string;
}

function SortableItem({ id }: SortableItemProps): ReactElement {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id,
  });
  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.6 : 1,
  };
  return (
    <li
      ref={setNodeRef}
      style={style}
      className="border-border bg-surface-1 flex items-center gap-3 rounded-md border p-3"
    >
      <button
        type="button"
        aria-label="Drag to reorder"
        className="text-fg-muted hover:text-fg cursor-grab touch-none"
        {...attributes}
        {...listeners}
      >
        <GripVertical className="size-4" aria-hidden="true" />
      </button>
      <span className="font-mono text-fg-muted text-xs">{id.slice(0, 8)}</span>
    </li>
  );
}

function ShelfDetailSkeleton(): ReactElement {
  return (
    <div className="mx-auto max-w-3xl px-6 py-10 sm:px-10">
      <Skeleton className="h-8 w-1/3" />
      <div className="mt-8 space-y-2">
        {[0, 1, 2].map((i) => (
          <Skeleton key={i} className="h-12 w-full" />
        ))}
      </div>
    </div>
  );
}
