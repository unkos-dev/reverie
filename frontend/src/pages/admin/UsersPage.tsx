/**
 * `/admin/users` — admin-only user management page.
 *
 * Shows all users in a table with role dropdown and child-status
 * toggle. Non-admin callers are redirected to `/library`.
 */
import { type ReactElement, useCallback } from "react";
import { Navigate } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { useAuthMe } from "@/hooks/useAuthMe";
import { queryKeys } from "@/lib/query/keys";
import { listUsers, updateUserRole, updateUserChildStatus } from "@/api/users";
import type { User, Role } from "@/api/users";
import { ApiError } from "@/api";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";

function UsersPage(): ReactElement {
  const { data: me, isLoading: meLoading } = useAuthMe();
  const queryClient = useQueryClient();

  const {
    data: users,
    isLoading,
    error,
  } = useQuery({
    queryKey: queryKeys.users.list(),
    queryFn: ({ signal }) => listUsers(signal),
    enabled: me?.role === "admin",
  });

  const roleMutation = useMutation({
    mutationFn: ({ id, role }: { id: string; role: Role }) => updateUserRole(id, role),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.users.all });
    },
    onError: (err: Error) => {
      const detail = err instanceof ApiError ? err.detail : err.message;
      toast.error(`Role update failed: ${detail}`);
    },
  });

  const childMutation = useMutation({
    mutationFn: ({ id, isChild }: { id: string; isChild: boolean }) =>
      updateUserChildStatus(id, isChild),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.users.all });
    },
    onError: (err: Error) => {
      const detail = err instanceof ApiError ? err.detail : err.message;
      toast.error(`Child status update failed: ${detail}`);
    },
  });

  const handleRoleChange = useCallback(
    (userId: string, role: Role) => {
      roleMutation.mutate({ id: userId, role });
    },
    [roleMutation],
  );

  const handleChildToggle = useCallback(
    (userId: string, isChild: boolean) => {
      childMutation.mutate({ id: userId, isChild });
    },
    [childMutation],
  );

  if (meLoading) {
    return (
      <div className="mx-auto max-w-4xl p-6">
        <Skeleton className="h-8 w-48" />
      </div>
    );
  }

  if (me?.role !== "admin") {
    return <Navigate to="/library" replace />;
  }

  return (
    <div className="mx-auto max-w-4xl p-6">
      <h1 className="mb-6 text-2xl font-bold text-fg">Users</h1>

      {isLoading && (
        <div className="space-y-2">
          {Array.from({ length: 3 }, (_, i) => (
            <Skeleton key={`skel-${String(i)}`} className="h-12 w-full" />
          ))}
        </div>
      )}

      {error && (
        <p className="text-destructive">
          Failed to load users: {error instanceof ApiError ? error.detail : error.message}
        </p>
      )}

      {users && users.length === 0 && (
        <p className="py-8 text-center text-muted-foreground">No users found.</p>
      )}

      {users && users.length > 0 && (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              <TableHead>Email</TableHead>
              <TableHead>Role</TableHead>
              <TableHead>Child</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {users.map((user) => (
              <UserRow
                key={user.id}
                user={user}
                isSelf={user.id === me.id}
                onRoleChange={handleRoleChange}
                onChildToggle={handleChildToggle}
              />
            ))}
          </TableBody>
        </Table>
      )}
    </div>
  );
}

interface UserRowProps {
  user: User;
  isSelf: boolean;
  onRoleChange: (userId: string, role: Role) => void;
  onChildToggle: (userId: string, isChild: boolean) => void;
}

function isRole(v: string): v is Role {
  return v === "admin" || v === "adult" || v === "child";
}

function UserRow({ user, isSelf, onRoleChange, onChildToggle }: UserRowProps): ReactElement {
  return (
    <TableRow>
      <TableCell className="font-medium">
        {user.display_name}
        {isSelf && (
          <Badge variant="outline" className="ml-2">
            you
          </Badge>
        )}
      </TableCell>
      <TableCell className="text-muted-foreground">{user.email ?? "—"}</TableCell>
      <TableCell>
        <Select
          value={user.role}
          onValueChange={(v) => {
            if (isRole(v)) onRoleChange(user.id, v);
          }}
        >
          <SelectTrigger className="w-28">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="admin">admin</SelectItem>
            <SelectItem value="adult">adult</SelectItem>
            <SelectItem value="child">child</SelectItem>
          </SelectContent>
        </Select>
      </TableCell>
      <TableCell>
        <Switch
          checked={user.is_child}
          onCheckedChange={(v) => {
            onChildToggle(user.id, v);
          }}
        />
      </TableCell>
    </TableRow>
  );
}

export { UsersPage };
