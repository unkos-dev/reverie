/**
 * `/admin/users` — admin-only user management page.
 *
 * Lists every user with controls to change role, toggle child status, create a
 * new account, disable or re-enable an account, and reset a password. Non-admin
 * callers are redirected to `/library`.
 */
import { type ReactElement, type SyntheticEvent, useCallback, useState } from "react";
import { Navigate } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { useAuthMe } from "@/hooks/useAuthMe";
import { queryKeys } from "@/lib/query/keys";
import {
  listUsers,
  updateUserRole,
  updateUserChildStatus,
  createUser,
  setAccountStatus,
  adminResetPassword,
} from "@/api/users";
import type { User, Role, CreateUserInput } from "@/api/users";
import { ApiError } from "@/api";
import { displayNameField, emailField, newPasswordField } from "@/api/auth.schemas";
import { formString } from "@/lib/form";
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
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

const ROLE_OPTIONS: readonly Role[] = ["admin", "adult", "child"];

function errorDetail(err: unknown): string {
  if (err instanceof ApiError) return err.detail;
  if (err instanceof Error) return err.message;
  return "Unknown error";
}

function UsersPage(): ReactElement {
  const { data: me, isLoading: meLoading, isError: meError } = useAuthMe();
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

  const invalidateUsers = useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: queryKeys.users.all });
  }, [queryClient]);

  const roleMutation = useMutation({
    mutationFn: ({ id, role }: { id: string; role: Role }) => updateUserRole(id, role),
    onSuccess: () => {
      invalidateUsers();
      // Invalidate the caller's own identity. staleTime: Infinity means it
      // never auto-refreshes, so a self-demotion would leave me.role='admin'
      // cached indefinitely without this explicit invalidation.
      void queryClient.invalidateQueries({ queryKey: queryKeys.auth.me() });
    },
    onError: (err: Error) => {
      toast.error(`Role update failed: ${errorDetail(err)}`);
    },
  });

  const childMutation = useMutation({
    mutationFn: ({ id, isChild }: { id: string; isChild: boolean }) =>
      updateUserChildStatus(id, isChild),
    onSuccess: () => {
      invalidateUsers();
      void queryClient.invalidateQueries({ queryKey: queryKeys.auth.me() });
    },
    onError: (err: Error) => {
      toast.error(`Child status update failed: ${errorDetail(err)}`);
    },
  });

  const statusMutation = useMutation({
    mutationFn: ({ id, disabled }: { id: string; disabled: boolean }) =>
      setAccountStatus(id, disabled),
    onSuccess: (user) => {
      invalidateUsers();
      toast.success(`${user.display_name} ${user.disabled ? "disabled" : "enabled"}.`);
    },
    onError: (err: Error) => {
      toast.error(`Account status update failed: ${errorDetail(err)}`);
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

  const handleStatusToggle = useCallback(
    (userId: string, disabled: boolean) => {
      statusMutation.mutate({ id: userId, disabled });
    },
    [statusMutation],
  );

  if (meLoading) {
    return (
      <div className="mx-auto max-w-4xl p-6">
        <Skeleton className="h-8 w-48" />
      </div>
    );
  }

  if (meError) {
    return (
      <div className="mx-auto max-w-4xl p-6">
        <p className="text-destructive">
          Could not verify your identity. Please refresh the page or try again later.
        </p>
      </div>
    );
  }

  if (me?.role !== "admin") {
    return <Navigate to="/library" replace />;
  }

  return (
    <div className="mx-auto max-w-4xl p-6">
      <div className="mb-6 flex items-center justify-between">
        <h1 className="text-2xl font-bold text-fg">Users</h1>
        <CreateUserDialog onCreated={invalidateUsers} />
      </div>

      {isLoading && (
        <div className="space-y-2">
          {Array.from({ length: 3 }, (_, i) => (
            <Skeleton key={`skel-${String(i)}`} className="h-12 w-full" />
          ))}
        </div>
      )}

      {error && <p className="text-destructive">Failed to load users: {errorDetail(error)}</p>}

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
              <TableHead>Status</TableHead>
              <TableHead className="text-right">Actions</TableHead>
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
                onStatusToggle={handleStatusToggle}
              />
            ))}
          </TableBody>
        </Table>
      )}
    </div>
  );
}

type UserRowProps = {
  user: User;
  isSelf: boolean;
  onRoleChange: (userId: string, role: Role) => void;
  onChildToggle: (userId: string, isChild: boolean) => void;
  onStatusToggle: (userId: string, disabled: boolean) => void;
};

function isRole(v: string): v is Role {
  return v === "admin" || v === "adult" || v === "child";
}

function UserRow({
  user,
  isSelf,
  onRoleChange,
  onChildToggle,
  onStatusToggle,
}: UserRowProps): ReactElement {
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
      <TableCell className="text-muted-foreground">{user.email ?? "no email"}</TableCell>
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
      <TableCell>
        {user.disabled ? (
          <Badge variant="destructive">disabled</Badge>
        ) : (
          <Badge variant="outline">active</Badge>
        )}
      </TableCell>
      <TableCell className="text-right">
        <div className="flex items-center justify-end gap-2">
          <ResetPasswordDialog userId={user.id} displayName={user.display_name} />
          {!isSelf && (
            <Button
              variant={user.disabled ? "outline" : "destructive"}
              size="sm"
              onClick={() => {
                onStatusToggle(user.id, !user.disabled);
              }}
            >
              {user.disabled ? "Enable" : "Disable"}
            </Button>
          )}
        </div>
      </TableCell>
    </TableRow>
  );
}

type CreateUserDialogProps = {
  onCreated: () => void;
};

function CreateUserDialog({ onCreated }: CreateUserDialogProps): ReactElement {
  const [open, setOpen] = useState(false);
  const [role, setRole] = useState<Role>("adult");
  const [error, setError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: (input: CreateUserInput) => createUser(input),
    onSuccess: (user) => {
      toast.success(`Created ${user.display_name}.`);
      onCreated();
      // Close through the same handler as a manual dismiss so the role selection
      // resets; a bare setOpen(false) would leave it on the last-created role.
      handleOpenChange(false);
    },
    onError: (err: Error) => {
      const detail = errorDetail(err);
      setError(detail);
      toast.error(`Create failed: ${detail}`);
    },
  });

  function handleOpenChange(next: boolean): void {
    setOpen(next);
    if (!next) {
      setError(null);
      setRole("adult");
    }
  }

  function handleSubmit(e: SyntheticEvent<HTMLFormElement>): void {
    e.preventDefault();
    setError(null);
    const data = new FormData(e.currentTarget);
    const displayName = displayNameField.safeParse(formString(data, "display_name"));
    if (!displayName.success) {
      setError("Enter a display name.");
      return;
    }
    const email = emailField.safeParse(formString(data, "email"));
    if (!email.success) {
      setError("Enter a valid email address.");
      return;
    }
    const password = newPasswordField.safeParse(formString(data, "password"));
    if (!password.success) {
      setError("Use at least 8 characters for the password.");
      return;
    }
    mutation.mutate({
      email: email.data,
      display_name: displayName.data,
      role,
      password: password.data,
    });
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogTrigger asChild>
        <Button>Create user</Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create user</DialogTitle>
          <DialogDescription>
            Set an initial password and hand it to the user out of band. They can change it after
            signing in.
          </DialogDescription>
        </DialogHeader>
        <form onSubmit={handleSubmit} className="flex flex-col gap-4" noValidate>
          <Field>
            <FieldLabel htmlFor="create-display-name">Display name</FieldLabel>
            <Input id="create-display-name" name="display_name" type="text" required />
          </Field>
          <Field>
            <FieldLabel htmlFor="create-email">Email</FieldLabel>
            <Input id="create-email" name="email" type="email" autoComplete="off" required />
          </Field>
          <Field>
            <FieldLabel htmlFor="create-role">Role</FieldLabel>
            <Select
              value={role}
              onValueChange={(v) => {
                if (isRole(v)) setRole(v);
              }}
            >
              <SelectTrigger id="create-role" className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {ROLE_OPTIONS.map((r) => (
                  <SelectItem key={r} value={r}>
                    {r}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field>
            <FieldLabel htmlFor="create-password">Initial password</FieldLabel>
            <Input
              id="create-password"
              name="password"
              type="password"
              autoComplete="new-password"
              required
            />
          </Field>
          {error !== null ? <FieldError>{error}</FieldError> : null}
          <DialogFooter>
            <DialogClose asChild>
              <Button type="button" variant="outline">
                Cancel
              </Button>
            </DialogClose>
            <Button type="submit" disabled={mutation.isPending}>
              Create
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

type ResetPasswordDialogProps = {
  userId: string;
  displayName: string;
};

function ResetPasswordDialog({ userId, displayName }: ResetPasswordDialogProps): ReactElement {
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: (newPassword: string) => adminResetPassword(userId, newPassword),
    onSuccess: () => {
      toast.success(`Password reset for ${displayName}.`);
      setOpen(false);
    },
    onError: (err: Error) => {
      const detail = errorDetail(err);
      setError(detail);
      toast.error(`Reset failed: ${detail}`);
    },
  });

  function handleOpenChange(next: boolean): void {
    setOpen(next);
    if (!next) setError(null);
  }

  function handleSubmit(e: SyntheticEvent<HTMLFormElement>): void {
    e.preventDefault();
    setError(null);
    const data = new FormData(e.currentTarget);
    const password = newPasswordField.safeParse(formString(data, "password"));
    if (!password.success) {
      setError("Use at least 8 characters for the password.");
      return;
    }
    mutation.mutate(password.data);
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogTrigger asChild>
        <Button variant="outline" size="sm">
          Reset password
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Reset password</DialogTitle>
          <DialogDescription>
            Set a new password for {displayName} and relay it out of band. Their existing sessions
            are signed out.
          </DialogDescription>
        </DialogHeader>
        <form onSubmit={handleSubmit} className="flex flex-col gap-4" noValidate>
          <Field>
            <FieldLabel htmlFor="reset-password">New password</FieldLabel>
            <Input
              id="reset-password"
              name="password"
              type="password"
              autoComplete="new-password"
              required
            />
          </Field>
          {error !== null ? <FieldError>{error}</FieldError> : null}
          <DialogFooter>
            <DialogClose asChild>
              <Button type="button" variant="outline">
                Cancel
              </Button>
            </DialogClose>
            <Button type="submit" disabled={mutation.isPending}>
              Reset
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export { UsersPage };
