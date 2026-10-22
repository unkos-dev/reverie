import {
  FieldError as AriaFieldError,
  Form,
  Input as AriaInput,
  Label as AriaLabel,
  TextField,
} from "react-aria-components";
/**
 * `/admin/users` — admin-only user management page.
 *
 * Lists every user with controls to change role, create an account, disable or
 * re-enable an account, and reset a password. Non-admin callers are redirected
 * to `/library`.
 */
import { type ReactElement, type SyntheticEvent, useCallback, useState } from "react";
import { Navigate } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { z } from "zod";

import { useAuthMe } from "@/hooks/useAuthMe";
import { queryKeys } from "@/lib/query/keys";
import {
  listUsers,
  updateUserRole,
  createUser,
  setAccountStatus,
  issueCredentialLink,
  cancelCredentialLink,
} from "@/api/users";
import type { User, Role, CreateUserInput, CredentialLink } from "@/api/users";
import { ApiError } from "@/api";
import { displayNameField, emailField } from "@/api/auth.schemas";
import { fetchSetupStatus } from "@/api/auth";
import { formatDateTime } from "@/lib/format/datetime";
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
} from "@/components/ui/dialog";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

const ROLE_OPTIONS: readonly Role[] = ["admin", "adult", "child"];
const roleField = z.enum(ROLE_OPTIONS);

function errorDetail(err: unknown): string {
  if (err instanceof ApiError) return err.detail;
  if (err instanceof Error) return err.message;
  return "Unknown error";
}

function UsersPage(): ReactElement {
  const {
    data: setup,
    isFetched: setupFetched,
    isFetching: setupFetching,
    refetch: refetchSetup,
  } = useQuery({
    queryKey: queryKeys.auth.setupStatus(),
    queryFn: ({ signal }) => fetchSetupStatus(signal),
    retry: false,
  });
  const localAuthEnabled = setup?.local_auth_enabled === true;
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

  const handleStatusToggle = useCallback(
    (userId: string, disabled: boolean) => {
      statusMutation.mutate({ id: userId, disabled });
    },
    [statusMutation],
  );

  if (meLoading) {
    return (
      <div className="mx-auto max-w-6xl p-6">
        <Skeleton className="h-8 w-48" />
      </div>
    );
  }

  if (meError) {
    return (
      <div className="mx-auto max-w-6xl p-6">
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
    <div className="mx-auto max-w-6xl p-6">
      <div className="mb-6 flex items-center justify-between">
        <h1 className="text-2xl font-bold text-fg">Users</h1>
        {localAuthEnabled ? <CreateUserDialog onCreated={invalidateUsers} /> : null}
      </div>

      {setup === undefined && !setupFetched ? (
        <p role="status" className="mb-6 text-muted-foreground">
          Loading authentication settings…
        </p>
      ) : null}
      {setup === undefined && setupFetched ? (
        <div className="mb-6 flex flex-col items-start gap-2">
          <p role="alert" className="text-destructive">
            Could not load authentication settings.
          </p>
          <Button
            type="button"
            variant="outline"
            size="sm"
            isDisabled={setupFetching}
            onPress={() => {
              void refetchSetup();
            }}
          >
            Retry authentication settings
          </Button>
        </div>
      ) : null}

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
                localAuthEnabled={localAuthEnabled}
                onLinkChange={invalidateUsers}
                onRoleChange={handleRoleChange}
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
  localAuthEnabled: boolean;
  onLinkChange: () => void;
  user: User;
  isSelf: boolean;
  onRoleChange: (userId: string, role: Role) => void;
  onStatusToggle: (userId: string, disabled: boolean) => void;
};

function isRole(v: string): v is Role {
  return v === "admin" || v === "adult" || v === "child";
}

function UserRow({
  localAuthEnabled,
  onLinkChange,
  user,
  isSelf,
  onRoleChange,
  onStatusToggle,
}: Readonly<UserRowProps>): ReactElement {
  const activeExpiry = user.credential_link_expires_at;
  const linkActive = activeExpiry !== null;
  const linkStatus = user.has_password ? "link pending" : "invited";
  const cancel = useMutation({
    mutationFn: () => cancelCredentialLink(user.id),
    onSuccess: () => {
      onLinkChange();
      toast.success("Link cancelled.");
    },
    onError: (err: Error) => {
      toast.error(errorDetail(err));
    },
  });
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
          aria-label="Role"
          value={user.role}
          onChange={(v) => {
            if (typeof v !== "string") return;
            if (isRole(v)) onRoleChange(user.id, v);
          }}
        >
          <SelectTrigger className="w-28">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem id="admin">admin</SelectItem>
            <SelectItem id="adult">adult</SelectItem>
            <SelectItem id="child">child</SelectItem>
          </SelectContent>
        </Select>
      </TableCell>
      <TableCell>
        <div className="flex flex-wrap gap-2">
          {user.disabled ? (
            <Badge variant="destructive">disabled</Badge>
          ) : (
            <Badge variant="outline">active</Badge>
          )}
          {linkActive ? (
            <Badge
              variant="outline"
              role="status"
              aria-label={`${linkStatus} expires ${formatDateTime(activeExpiry)}`}
            >
              {linkStatus}
            </Badge>
          ) : null}
        </div>
      </TableCell>
      <TableCell className="text-right">
        <div className="flex items-center justify-end gap-2">
          {localAuthEnabled && !isSelf ? (
            <ResetPasswordDialog user={user} linkActive={linkActive} onIssued={onLinkChange} />
          ) : null}
          {localAuthEnabled && linkActive ? (
            <Button
              variant="outline"
              size="sm"
              isDisabled={cancel.isPending}
              onPress={() => {
                cancel.mutate();
              }}
            >
              Cancel link
            </Button>
          ) : null}
          {!isSelf && (
            <Button
              variant={user.disabled ? "outline" : "destructive"}
              size="sm"
              onPress={() => {
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

type LinkResult = CredentialLink & { name: string; newAccount: boolean };

function LinkResultDialog({
  result,
  onDone,
}: Readonly<{ result: LinkResult; onDone: () => void }>): ReactElement {
  const url = `${window.location.origin}/set-password#${result.token}`;
  const expiry = formatDateTime(result.expires_at);
  const description = result.newAccount
    ? `${result.name} uses this link to choose a password. It expires ${expiry}. You won't see it again. If it's lost, create a new link from the user list.`
    : `${result.name} uses this link to choose a new password. It expires ${expiry}. You won't see it again.`;
  async function copyLink(): Promise<void> {
    try {
      await navigator.clipboard.writeText(url);
      toast.success("Link copied.");
    } catch {
      toast.error("Couldn't copy the link. Select the link and copy it manually.");
    }
  }
  return (
    <Dialog
      isOpen
      onOpenChange={(open) => {
        if (!open) onDone();
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Send this link to {result.name}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <Input readOnly value={url} aria-label="Credential link" />
        <DialogFooter>
          <Button
            variant="outline"
            onPress={() => {
              void copyLink();
            }}
          >
            Copy link
          </Button>
          <Button onPress={onDone}>Done</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function validateDisplayName(value: string): string | null {
  return displayNameField.safeParse(value).success ? null : "Enter a display name.";
}

function validateEmail(value: string): string | null {
  return emailField.safeParse(value).success ? null : "Enter a valid email address.";
}

function CreateUserDialog({ onCreated }: Readonly<{ onCreated: () => void }>): ReactElement {
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<LinkResult | null>(null);
  const mutation = useMutation({
    mutationFn: (input: CreateUserInput) => createUser(input),
    onSuccess: ({ user, link }) => {
      onCreated();
      handleOpenChange(false);
      setResult({ ...link, name: user.display_name, newAccount: true });
    },
    onError: (err: Error) => {
      setError(errorDetail(err));
    },
  });
  function handleOpenChange(next: boolean): void {
    setOpen(next);
    if (!next) {
      setError(null);
    }
  }
  function handleSubmit(event: SyntheticEvent<HTMLFormElement>): void {
    event.preventDefault();
    setError(null);
    const data = new FormData(event.currentTarget);
    const displayName = displayNameField.safeParse(formString(data, "display_name"));
    const email = emailField.safeParse(formString(data, "email"));
    if (!displayName.success || !email.success) return;
    const role = roleField.safeParse(data.get("role"));
    if (!role.success) {
      setError("Choose a valid role.");
      return;
    }
    mutation.mutate({ email: email.data, display_name: displayName.data, role: role.data });
  }
  return (
    <>
      <Dialog isOpen={open} onOpenChange={handleOpenChange}>
        <Button>Create user</Button>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Create user</DialogTitle>
            <DialogDescription>
              Create the account, then send the person the link it gives you. They choose their own
              password.
            </DialogDescription>
          </DialogHeader>
          <Form onSubmit={handleSubmit} className="flex flex-col gap-4">
            <TextField
              name="display_name"
              type="text"
              isRequired
              validate={validateDisplayName}
              render={(props) => <Field {...props} />}
            >
              <AriaLabel render={(props) => <FieldLabel {...props} />}>Display name</AriaLabel>
              <AriaInput render={(props) => <Input {...props} />} />
              <AriaFieldError className="text-destructive text-sm font-normal" />
            </TextField>
            <TextField
              name="email"
              type="email"
              autoComplete="off"
              isRequired
              validate={validateEmail}
              render={(props) => <Field {...props} />}
            >
              <AriaLabel render={(props) => <FieldLabel {...props} />}>Email</AriaLabel>
              <AriaInput render={(props) => <Input {...props} />} />
              <AriaFieldError className="text-destructive text-sm font-normal" />
            </TextField>
            <Field>
              <Select className="flex flex-col gap-2" name="role" defaultValue="adult">
                <AriaLabel className="flex w-fit gap-2 text-sm font-medium leading-snug">
                  Role
                </AriaLabel>
                <SelectTrigger id="create-role" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {ROLE_OPTIONS.map((value) => (
                    <SelectItem key={value} id={value}>
                      {value}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            {error !== null ? <FieldError>{error}</FieldError> : null}
            <DialogFooter>
              <DialogClose type="button">Cancel</DialogClose>
              <Button type="submit" isPending={mutation.isPending}>
                Create user
              </Button>
            </DialogFooter>
          </Form>
        </DialogContent>
      </Dialog>
      {result !== null ? (
        <LinkResultDialog
          result={result}
          onDone={() => {
            setResult(null);
            mutation.reset();
          }}
        />
      ) : null}
    </>
  );
}

function ResetPasswordDialog({
  user,
  linkActive,
  onIssued,
}: Readonly<{ user: User; linkActive: boolean; onIssued: () => void }>): ReactElement {
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<LinkResult | null>(null);
  const mutation = useMutation({
    mutationFn: () => issueCredentialLink(user.id),
    onSuccess: (link) => {
      setOpen(false);
      onIssued();
      setResult({ ...link, name: user.display_name, newAccount: false });
    },
    onError: (err: Error) => {
      setError(
        err instanceof ApiError && err.detail === "the account has no email address"
          ? `Add an email address to ${user.display_name} before creating a link.`
          : errorDetail(err),
      );
    },
  });
  return (
    <>
      <Dialog
        isOpen={open}
        onOpenChange={(next) => {
          setOpen(next);
          if (!next) setError(null);
        }}
      >
        <Button variant="outline" size="sm">
          Reset password
        </Button>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Reset password for {user.display_name}?</DialogTitle>
            <DialogDescription>
              This creates a link {user.display_name} can use to choose a new password. Their
              current password keeps working until the link is used. Using it signs them out
              everywhere.{linkActive ? " This replaces the link created earlier." : ""}
            </DialogDescription>
          </DialogHeader>
          {error !== null ? <FieldError>{error}</FieldError> : null}
          <DialogFooter>
            <DialogClose type="button">Cancel</DialogClose>
            <Button
              isDisabled={mutation.isPending}
              onPress={() => {
                setError(null);
                mutation.mutate();
              }}
            >
              Create link
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      {result !== null ? (
        <LinkResultDialog
          result={result}
          onDone={() => {
            setResult(null);
            mutation.reset();
          }}
        />
      ) : null}
    </>
  );
}

export { UsersPage };
