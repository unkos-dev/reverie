import { Form, Label as AriaLabel } from "react-aria-components";
/**
 * `/admin/users` — admin-only user management page.
 *
 * Lists every user with controls to change role, create an account, disable or
 * re-enable an account, and reset a password. Non-admin callers are redirected
 * to `/library`.
 */
import { type ReactElement, useActionState, useCallback, useState } from "react";
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
  sendCredentialLinkEmail,
  fetchCredentialLinkEmail,
} from "@/api/users";
import type { User, Role, CreateUserInput, CredentialLink } from "@/api/users";
import { ApiError } from "@/api";
import {
  displayNameField,
  emailField,
  validateDisplayName,
  validateEmail,
} from "@/api/auth.schemas";
import { fetchSetupStatus, fetchEmailVerification } from "@/api/auth";
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
import { Field, FieldError, TextInputField } from "@/components/ui/field";
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
              <TableHead scope="col">Name</TableHead>
              <TableHead scope="col">Email</TableHead>
              <TableHead scope="col">Role</TableHead>
              <TableHead scope="col">Status</TableHead>
              <TableHead scope="col" className="text-right">
                Actions
              </TableHead>
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

type LinkResult = CredentialLink & {
  name: string;
  newAccount: boolean;
  userId: string;
  email: string | null;
  hasPassword: boolean;
};

const mailFailureReasons = {
  connection: "couldn't connect to the mail server",
  tls: "the secure connection to the mail server failed",
  authentication: "the mail server rejected Reverie's sign-in",
  temporary_refusal: "the mail server refused it for now",
  permanent_refusal: "the mail server refused it",
  timeout: "the mail server didn't respond in time",
  payload_unreadable: "the queued message couldn't be read",
};

function LinkResultContent({
  result,
  onDone,
}: Readonly<{ result: LinkResult; onDone: () => void }>): ReactElement {
  const route = result.hasPassword ? "reset-password" : "set-password";
  const url = `${window.location.origin}/${route}#${result.token}`;
  const availability = useQuery({
    queryKey: queryKeys.auth.emailVerification(),
    queryFn: ({ signal }) => fetchEmailVerification(signal),
    retry: false,
  });
  const sending = useMutation({
    mutationFn: () => sendCredentialLinkEmail(result.userId, result.token),
    gcTime: 0,
  });
  const jobId = sending.data?.job_id;
  const delivery = useQuery({
    queryKey: queryKeys.users.linkEmail(result.userId, jobId ?? ""),
    queryFn: ({ signal }) => fetchCredentialLinkEmail(result.userId, jobId ?? "", signal),
    enabled: jobId !== undefined,
    initialData: sending.data,
    gcTime: 0,
    staleTime: Infinity,
    refetchInterval: (query) => {
      if (query.state.status === "error") return false;
      return query.state.data?.status === "queued" ? 3000 : false;
    },
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
    retry: false,
  });
  const status = delivery.data ?? sending.data;
  const inactive =
    status?.status === "inactive" ||
    (sending.error instanceof ApiError && sending.error.status === 422);
  const accepted = status?.status === "accepted";
  const queued =
    sending.isPending || delivery.isFetching || (!delivery.isError && status?.status === "queued");
  let mailMessage: string | null = null;
  if (inactive) mailMessage = "This link is no longer active. Create a new link to send it.";
  else if (sending.error instanceof ApiError && sending.error.status === 503)
    mailMessage = "Email isn't available right now. Copy the link instead.";
  else if (sending.isError || delivery.isError)
    mailMessage = "Couldn't confirm the request. Try again.";
  else if (status?.status === "failed")
    mailMessage = `Sending failed: ${mailFailureReasons[status.failure_category]}. Copy the link instead, or try again.`;
  else if (accepted)
    mailMessage = `The mail server accepted the email for ${result.email ?? ""}. This doesn't confirm it reached the inbox.`;
  else if (queued) mailMessage = `Sending to ${result.email ?? ""}…`;
  const expiry = formatDateTime(result.expires_at);
  const description = result.newAccount
    ? `${result.name} uses this link to choose a password. It expires ${expiry}. You won't see it again. If it's lost, create a new link from the user list.`
    : `${result.name} uses this link to choose a new password. It expires ${expiry}. You won't see it again.`;
  const sendLabel =
    status?.status === "failed" || sending.isError || delivery.isError ? "Try again" : "Send email";
  async function copyLink(): Promise<void> {
    try {
      await navigator.clipboard.writeText(url);
      toast.success("Link copied.");
    } catch {
      toast.error("Couldn't copy the link. Select the link and copy it manually.");
    }
  }
  return (
    <>
      <DialogHeader>
        <DialogTitle>Send this link to {result.name}</DialogTitle>
        <DialogDescription>{description}</DialogDescription>
      </DialogHeader>
      <Input readOnly value={url} aria-label="Credential link" />
      {mailMessage !== null ? (
        <p role="status" className="mt-4 text-sm">
          {mailMessage}
        </p>
      ) : null}
      <DialogFooter>
        {availability.data?.mail_enabled === true &&
        result.email !== null &&
        !inactive &&
        !accepted ? (
          <Button
            isPending={queued}
            onPress={() => {
              if (delivery.isError) void delivery.refetch();
              else sending.mutate();
            }}
          >
            {sendLabel}
          </Button>
        ) : null}
        <Button
          autoFocus
          variant="outline"
          onPress={() => {
            void copyLink();
          }}
        >
          Copy link
        </Button>
        <Button onPress={onDone}>Done</Button>
      </DialogFooter>
    </>
  );
}

function useLinkDialogState() {
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<LinkResult | null>(null);
  function changeOpen(resetMutation: () => void): (next: boolean) => void {
    return (next: boolean): void => {
      setOpen(next);
      if (!next) {
        setError(null);
        setResult(null);
        if (result !== null) resetMutation();
      }
    };
  }
  return { open, error, result, setOpen, setError, setResult, changeOpen };
}

function CreateUserDialog({ onCreated }: Readonly<{ onCreated: () => void }>): ReactElement {
  const { open, error, result, setOpen, setError, setResult, changeOpen } = useLinkDialogState();
  const mutation = useMutation({
    mutationFn: (input: CreateUserInput) => createUser(input),
    gcTime: 0,
    onSuccess: ({ user, link }) => {
      onCreated();
      setOpen(true);
      setResult({
        ...link,
        name: user.display_name,
        newAccount: true,
        userId: user.id,
        email: user.email,
        hasPassword: user.has_password,
      });
    },
    onError: (err: Error) => {
      setError(errorDetail(err));
    },
  });
  const handleOpenChange = changeOpen(mutation.reset);
  async function submit(data: FormData): Promise<string | null> {
    if (mutation.isPending) return null;
    setError(null);
    const displayName = displayNameField.safeParse(formString(data, "display_name"));
    const email = emailField.safeParse(formString(data, "email"));
    if (!displayName.success || !email.success) return null;
    const role = roleField.safeParse(data.get("role"));
    if (!role.success) return "Choose a valid role.";
    try {
      await mutation.mutateAsync({
        email: email.data,
        display_name: displayName.data,
        role: role.data,
      });
      return null;
    } catch (err) {
      return errorDetail(err);
    }
  }
  return (
    <Dialog isOpen={open} onOpenChange={handleOpenChange}>
      <Button>Create user</Button>
      <DialogContent>
        {result !== null ? (
          <LinkResultContent
            result={result}
            onDone={() => {
              handleOpenChange(false);
            }}
          />
        ) : (
          <>
            <DialogHeader>
              <DialogTitle>Create user</DialogTitle>
              <DialogDescription>
                Create the account, then send the person the link it gives you. They choose their
                own password.
              </DialogDescription>
            </DialogHeader>
            <CreateUserForm submit={submit} pending={mutation.isPending} serverError={error} />
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}

function CreateUserForm({
  submit,
  pending,
  serverError,
}: Readonly<{
  serverError: string | null;
  submit: (data: FormData) => Promise<string | null>;
  pending: boolean;
}>): ReactElement {
  const [state, action, actionPending] = useActionState<
    { values: Partial<Record<string, string>>; error: string | null },
    FormData
  >(
    async (
      _previous: { values: Partial<Record<string, string>>; error: string | null },
      data: FormData,
    ) => {
      const values = z.record(z.string(), z.string()).parse(Object.fromEntries(data));
      return { values, error: await submit(data) };
    },
    { values: {}, error: null },
  );
  const error = actionPending ? null : (state.error ?? serverError);
  return (
    <Form action={action} validationErrors={{}} className="flex flex-col gap-4">
      <TextInputField
        name="display_name"
        type="text"
        isRequired
        validate={validateDisplayName}
        label="Display name"
        defaultValue={state.values.display_name}
      />
      <TextInputField
        name="email"
        type="email"
        autoComplete="off"
        isRequired
        validate={validateEmail}
        label="Email"
        defaultValue={state.values.email}
      />
      <Field>
        <Select
          className="flex flex-col gap-2"
          name="role"
          defaultValue={state.values.role ?? "adult"}
        >
          <AriaLabel className="flex w-fit gap-2 text-sm font-medium leading-snug">Role</AriaLabel>
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
        <Button type="submit" isPending={pending || actionPending}>
          Create user
        </Button>
      </DialogFooter>
    </Form>
  );
}

function ResetPasswordDialog({
  user,
  linkActive,
  onIssued,
}: Readonly<{ user: User; linkActive: boolean; onIssued: () => void }>): ReactElement {
  const { open, error, result, setOpen, setError, setResult, changeOpen } = useLinkDialogState();
  const mutation = useMutation({
    mutationFn: () => issueCredentialLink(user.id),
    gcTime: 0,
    onSuccess: (link) => {
      onIssued();
      setOpen(true);
      setResult({
        ...link,
        name: user.display_name,
        newAccount: false,
        userId: user.id,
        email: user.email,
        hasPassword: user.has_password,
      });
    },
    onError: (err: Error) => {
      setError(
        err instanceof ApiError && err.detail === "the account has no email address"
          ? `Add an email address to ${user.display_name} before creating a link.`
          : errorDetail(err),
      );
    },
  });
  const handleOpenChange = changeOpen(mutation.reset);
  return (
    <Dialog isOpen={open} onOpenChange={handleOpenChange}>
      <Button variant="outline" size="sm">
        Reset password
      </Button>
      <DialogContent>
        {result !== null ? (
          <LinkResultContent
            result={result}
            onDone={() => {
              handleOpenChange(false);
            }}
          />
        ) : (
          <>
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
                isPending={mutation.isPending}
                onPress={() => {
                  setError(null);
                  mutation.mutate();
                }}
              >
                Create link
              </Button>
            </DialogFooter>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}

export { UsersPage };
