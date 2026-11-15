import { useLayoutEffect, useRef, useState, type ReactElement } from "react";
import type { Shelf } from "@/api";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem } from "@/components/ui/dropdown-menu";
import { Link } from "@/components/ui/link";

type Props = {
  shelves: readonly Shelf[];
  selected: readonly string[];
  onSelect: (id: string | null) => void;
};

export function ShelfTabs({ shelves, selected, onSelect }: Readonly<Props>): ReactElement {
  const container = useRef<HTMLDivElement>(null);
  const measure = useRef<HTMLDivElement>(null);
  const [visibleCount, setVisibleCount] = useState(shelves.length);
  useLayoutEffect(() => {
    const element = container.current;
    const labels = measure.current;
    if (element === null || labels === null) return;
    const resize = new ResizeObserver(() => {
      if (window.matchMedia("(max-width: 639px)").matches) {
        setVisibleCount(shelves.length);
        return;
      }
      const widths = [...labels.children].map((child) => child.getBoundingClientRect().width + 4);
      let used = widths[0] ?? 0;
      let count = 0;
      const total = widths.reduce((sum, width) => sum + width, 0);
      const available = element.clientWidth - (total > element.clientWidth ? 80 : 0);
      for (const width of widths.slice(1)) {
        if (used + width > available) break;
        used += width;
        count += 1;
      }
      setVisibleCount(count);
    });
    resize.observe(element);
    resize.observe(labels);
    return () => {
      resize.disconnect();
    };
  }, [shelves]);
  const visible = shelves.slice(0, visibleCount);
  const overflow = shelves.slice(visibleCount);
  return (
    <div
      ref={container}
      className="border-hairline relative flex min-w-0 shrink-0 items-center overflow-hidden gap-1 border-b py-2.5"
    >
      <div
        ref={measure}
        aria-hidden="true"
        className="pointer-events-none invisible absolute flex w-max whitespace-nowrap text-sm"
      >
        <span className="inline-flex items-center gap-2 border border-transparent px-3 font-medium">
          All books
        </span>
        {shelves.map((shelf) => (
          <span
            key={shelf.id}
            className="inline-flex items-center gap-2 border border-transparent px-3 font-medium"
          >
            {shelf.name}
            <span>{shelf.item_count}</span>
          </span>
        ))}
      </div>
      <nav
        aria-label="Library shelves"
        className="flex min-w-0 flex-1 gap-1 overflow-x-auto overflow-y-clip"
      >
        <Link
          aria-current={selected.length === 0 ? "page" : undefined}
          onPress={() => {
            onSelect(null);
          }}
          className="h-8 shrink-0 px-3 whitespace-nowrap"
        >
          All books
        </Link>
        {visible.map((shelf) => (
          <Link
            key={shelf.id}
            aria-label={`${shelf.name}, ${String(shelf.item_count)} ${shelf.item_count === 1 ? "book" : "books"}`}
            aria-current={selected.length === 1 && selected[0] === shelf.id ? "page" : undefined}
            onPress={() => {
              onSelect(shelf.id);
            }}
            className="h-8 shrink-0 px-3 whitespace-nowrap"
          >
            {shelf.name} <span className="text-fg-muted tabular-nums">{shelf.item_count}</span>
          </Link>
        ))}
      </nav>
      {overflow.length > 0 ? (
        <DropdownMenu>
          <Button variant="ghost" size="sm">
            More
          </Button>
          <DropdownMenuContent>
            {overflow.map((shelf) => (
              <DropdownMenuItem
                key={shelf.id}
                textValue={shelf.name}
                aria-label={`${shelf.name}, ${String(shelf.item_count)} ${shelf.item_count === 1 ? "book" : "books"}`}
                onAction={() => {
                  onSelect(shelf.id);
                }}
              >
                {shelf.name} <span className="text-fg-muted tabular-nums">{shelf.item_count}</span>
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      ) : null}
    </div>
  );
}
