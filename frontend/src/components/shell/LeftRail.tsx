/**
 * Left rail — the app's global navigation (spec §3, UNK-385).
 *
 * Sticky full-height `canvas-2` column: brand lockup, primary nav
 * (live + disabled-placeholder entries), the user's shelves nested
 * under Shelves, a role-gated admin cluster, and the user chip.
 *
 * Role gating is cosmetic shape, not authorization — the backend 403s
 * non-admins on every admin route and API regardless of what this rail
 * renders (spec S6). The cluster stays absent while authn is pending
 * (`data === undefined`) so non-admins never see an admin flash.
 */
import { useQuery } from "@tanstack/react-query";
import type { ReactElement } from "react";
import { Link, NavLink } from "react-router";

import { listShelves, type Shelf } from "@/api";
import { Lockup } from "@/components/Lockup";
import { Separator } from "@/components/ui/separator";
import { useAuthMe } from "@/hooks/useAuthMe";
import { queryKeys } from "@/lib/query/keys";
import { useTheme } from "@/lib/theme/ThemeProvider";
import { cn } from "@/lib/utils";

import { ADMIN_NAV, PLANNED_TOOLTIP, PRIMARY_NAV, type NavItem } from "./nav-items";
import { UserChip } from "./UserMenu";

/** Max shelf rows in the rail before the "All shelves" overflow link. */
const SHELF_CAP = 7;

/** Global navigation rail (left edge of the app shell). */
export function LeftRail(): ReactElement {
  const { effective } = useTheme();
  const { data: me } = useAuthMe();
  const { data: shelves } = useQuery({
    queryKey: queryKeys.shelves.list(),
    queryFn: ({ signal }) => listShelves(signal),
    staleTime: 60_000,
  });

  return (
    <aside
      aria-label="Navigation"
      className="bg-canvas-2 border-border flex h-full min-h-0 w-full flex-col border-r"
    >
      <div className="px-5 pb-4 pt-6">
        <Link
          to="/library"
          viewTransition
          className="focus-visible:ring-accent inline-flex rounded-sm focus-visible:outline-none focus-visible:ring-2"
        >
          <Lockup size={13} theme={effective} />
        </Link>
      </div>
      <nav aria-label="Primary" className="flex-1 overflow-y-auto px-3 pb-4">
        <ul className="flex flex-col gap-0.5">
          {PRIMARY_NAV.map((item) => (
            <li key={item.label}>
              <RailItem item={item} />
              {item.label === "Shelves" ? <ShelfList shelves={shelves} /> : null}
            </li>
          ))}
        </ul>
        {me?.role === "admin" ? (
          <>
            <Separator className="my-3" />
            <div role="group" aria-label="Admin">
              <ul className="flex flex-col gap-0.5">
                {ADMIN_NAV.map((item) => (
                  <li key={item.label}>
                    <RailItem item={item} />
                  </li>
                ))}
              </ul>
            </div>
          </>
        ) : null}
      </nav>
      <div className="border-border border-t p-3">
        <UserChip />
      </div>
    </aside>
  );
}

const ITEM_BASE =
  "relative flex w-full items-center gap-2 rounded-md px-3 py-2 text-sm font-body " +
  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent";

interface RailItemProps {
  item: NavItem;
}

/**
 * One primary-nav row. Live entries are `NavLink`s with the
 * surface-pill + gold slot-bar active treatment; disabled entries are
 * spec-S3 placeholders — visible, `aria-disabled`, out of tab order,
 * described by the "planned" tooltip (spec §9.4).
 */
function RailItem({ item }: RailItemProps): ReactElement {
  if (item.disabled === true || item.to === undefined) {
    const descId = `nav-planned-${item.label.toLowerCase()}`;
    return (
      <span
        aria-disabled="true"
        aria-describedby={descId}
        className={cn(ITEM_BASE, "text-fg-faint group cursor-default font-normal")}
      >
        {item.label}
        <span
          id={descId}
          role="tooltip"
          className="border-border bg-surface text-fg-muted pointer-events-none absolute left-full top-1/2 z-10 ml-2 -translate-y-1/2 whitespace-nowrap rounded-sm border px-2 py-1 text-xs opacity-0 transition-opacity group-hover:opacity-100"
        >
          {PLANNED_TOOLTIP}
        </span>
      </span>
    );
  }
  return (
    <NavLink
      to={item.to}
      viewTransition
      className={({ isActive }) =>
        cn(
          ITEM_BASE,
          "text-fg-muted hover:bg-surface hover:text-fg font-medium transition-colors",
          isActive && "bg-surface text-fg",
        )
      }
    >
      {({ isActive }) => (
        <>
          {/* 16×2px gold slot bar — the glyph's slot as active marker. */}
          <span
            aria-hidden="true"
            className={cn("bg-accent h-[2px] w-4 shrink-0", !isActive && "opacity-0")}
          />
          {item.label}
        </>
      )}
    </NavLink>
  );
}

interface ShelfListProps {
  shelves: Shelf[] | undefined;
}

/**
 * Shelf rows nested under the Shelves entry (spec S4). Renders nothing
 * until the shelves query resolves — the nav is functional without
 * counts, and a fetch failure simply leaves the rail shelf-less.
 */
function ShelfList({ shelves }: ShelfListProps): ReactElement | null {
  if (shelves === undefined || shelves.length === 0) return null;
  const visible = shelves.slice(0, SHELF_CAP);
  return (
    <ul className="mb-1 mt-0.5 flex flex-col gap-0.5">
      {visible.map((shelf) => (
        <li key={shelf.id}>
          <NavLink
            to={`/shelves/${shelf.id}`}
            viewTransition
            className={({ isActive }) =>
              cn(
                "text-fg-muted hover:bg-surface hover:text-fg flex items-center justify-between gap-2 rounded-md py-1.5 pl-9 pr-3 text-[13px] font-normal transition-colors",
                "focus-visible:ring-accent focus-visible:outline-none focus-visible:ring-2",
                isActive && "bg-surface text-fg",
              )
            }
          >
            <span className="truncate">{shelf.name}</span>
            {/* Fixed-width mono slot so late-arriving counts don't shift layout. */}
            <span className="text-fg-faint w-7 shrink-0 text-right font-mono text-xs tabular-nums">
              {shelf.item_count}
            </span>
          </NavLink>
        </li>
      ))}
      {shelves.length > SHELF_CAP ? (
        <li>
          <Link
            to="/shelves"
            viewTransition
            className="text-fg-faint hover:text-fg focus-visible:ring-accent flex rounded-md py-1.5 pl-9 pr-3 text-[13px] transition-colors focus-visible:outline-none focus-visible:ring-2"
          >
            All shelves →
          </Link>
        </li>
      ) : null}
    </ul>
  );
}
