/**
 * `/admin/settings/:area` — admin-only instance settings, one page per area.
 *
 * Owns the shared settings snapshot and the page chrome; each area's draft
 * lives in {@link SettingsWorkspace}, which is remounted when the area changes.
 */
import { useQuery, useQueryClient } from "@tanstack/react-query";
import type { ReactElement, ReactNode } from "react";
import { Link, Navigate, useParams } from "react-router";

import { ApiError } from "@/api/errors";
import { useAuthMe } from "@/hooks/useAuthMe";
import { queryKeys } from "@/lib/query/keys";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

import { Note, NoteText } from "./Notices";
import { SettingsWorkspace } from "./SettingsWorkspace";
import { AREA_IDS, AreaIdSchema, areaById } from "./fields";
import { reloadStatusQueryOptions, settingsQueryOptions } from "./queries";
import { formatClockWithAge, formatStamp } from "./time";

function StatusItem({ label, children }: { label: string; children: ReactNode }): ReactElement {
  return (
    <span>
      <span className="text-fg-faint mr-1.5 font-mono text-xs uppercase tracking-[0.04em]">
        {label}
      </span>
      {children}
    </span>
  );
}

function StatusLine({
  updatedAt,
  reload,
}: {
  updatedAt: string;
  reload: string | null | undefined;
}): ReactElement {
  return (
    <div className="text-fg-muted mt-3 flex flex-wrap gap-4 text-sm leading-normal">
      <StatusItem label="Last changed">{formatStamp(updatedAt)}</StatusItem>
      {reload === undefined ? null : (
        <StatusItem label="Server cache refreshed">
          {reload === null
            ? "Not refreshed since the server started. Values are read straight from the database, so this is normal."
            : formatClockWithAge(reload, new Date())}
        </StatusItem>
      )}
    </div>
  );
}

function LoadingRegion(): ReactElement {
  return (
    <div role="status" aria-busy="true" className="mt-4">
      <p className="text-fg-muted mt-6 text-[13px]">Loading settings</p>
      {[0, 1, 2].map((i) => (
        <div key={`section-${String(i)}`} className="border-border border-t pt-6">
          <Skeleton className="motion-reduce:animate-none h-[26px] w-[200px]" />
          <Skeleton className="motion-reduce:animate-none mt-2.5 h-3.5 w-[420px] max-w-full" />
          {[0, 1, 2].map((j) => (
            <div
              key={`row-${String(j)}`}
              className="grid grid-cols-[minmax(0,24rem)_minmax(0,1fr)] gap-8 py-4"
            >
              <div>
                <Skeleton className="motion-reduce:animate-none h-4 w-[240px] max-w-full" />
                <Skeleton className="motion-reduce:animate-none mt-2 h-3 w-[340px] max-w-full" />
              </div>
              <Skeleton className="motion-reduce:animate-none h-8 w-[120px]" />
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

export function SettingsPage(): ReactElement {
  const params = useParams();
  const queryClient = useQueryClient();
  const { data: me, isLoading: meLoading, isError: meError } = useAuthMe();
  const admin = me?.role === "admin";
  const snapshot = useQuery({ ...settingsQueryOptions(), enabled: admin });
  const reload = useQuery({ ...reloadStatusQueryOptions(), enabled: admin });

  const parsedArea = AreaIdSchema.safeParse(params.area);
  if (!parsedArea.success) return <Navigate to={`/admin/settings/${AREA_IDS[0]}`} replace />;
  const area = parsedArea.data;

  const forbidden =
    (me !== undefined && !admin) ||
    (snapshot.error instanceof ApiError && snapshot.error.status === 403);

  let body: ReactElement;
  let status: ReactElement | null = null;
  if (meError) {
    body = (
      <LoadError
        retry={() => {
          void queryClient.invalidateQueries({ queryKey: queryKeys.auth.me() });
        }}
      />
    );
  } else if (forbidden) {
    body = <ForbiddenNote />;
  } else if (meLoading || me === undefined || snapshot.isPending) {
    body = <LoadingRegion />;
  } else if (snapshot.data === undefined) {
    body = (
      <LoadError
        retry={() => {
          void snapshot.refetch();
        }}
      />
    );
  } else {
    status = (
      <StatusLine
        updatedAt={snapshot.data.updatedAt}
        reload={reload.isSuccess ? reload.data : undefined}
      />
    );
    body = <SettingsWorkspace key={area} area={area} base={snapshot.data} />;
  }

  return (
    <div className="mx-auto max-w-[1180px] px-10 pt-6">
      <div className="text-fg-faint mb-2 font-mono text-xs uppercase tracking-[0.04em]">
        Admin / Settings / {areaById(area).title}
      </div>
      <h1 className="font-display text-[30px] font-medium leading-[1.15] tracking-[-0.012em]">
        Settings
      </h1>
      <p className="text-fg-muted mt-1.5 max-w-[70ch]">
        Instance settings for this library. They apply to everyone who uses it, and they take effect
        without a restart.
      </p>
      {status}
      {body}
    </div>
  );
}

function LoadError({ retry }: { retry: () => void }): ReactElement {
  return (
    <Note
      title="Settings could not be loaded"
      role="alert"
      strong
      actions={
        <Button type="button" variant="outline" onClick={retry}>
          Try again
        </Button>
      }
    >
      <NoteText>
        The server did not respond, or it returned an error. Nothing has been changed.
      </NoteText>
    </Note>
  );
}

function ForbiddenNote(): ReactElement {
  return (
    <Note
      title="Settings are for administrators"
      role="alert"
      strong
      actions={
        <Button asChild variant="outline">
          <Link to="/library">Back to the library</Link>
        </Button>
      }
    >
      <NoteText>
        Your account does not have administrator access. Ask an administrator if something needs
        changing.
      </NoteText>
    </Note>
  );
}
