import { Form, Input as AriaInput, Label as AriaLabel, TextField } from "react-aria-components";
/**
 * `/tokens` — self-service device-token management.
 *
 * Lists the caller's own active tokens with revoke, and mints new ones
 * through an access-level + expiry dialog. Reachable by any authenticated
 * user; the `admin` level only renders for `me.role === 'admin'`, and the
 * backend re-enforces this ceiling regardless of what the dialog offers.
 *
 * Access levels map to the scope hierarchy (`read` < `write` < `admin`): a
 * higher level subsumes the lower ones, so the dialog offers a single level
 * rather than independent scope toggles.
 */
import { type ReactElement, useActionState, useCallback, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Copy } from "lucide-react";
import { toast } from "sonner";
import { z } from "zod";

import { useAuthMe } from "@/hooks/useAuthMe";
import { queryKeys } from "@/lib/query/keys";
import { listTokens, createToken, revokeToken } from "@/api/tokens";
import type { Token, Scope, CreateTokenResponse } from "@/api/tokens";
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
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldError, FieldLabel, TextInputField } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

const EXPIRY_OPTIONS = [
  { value: "30", label: "30 days" },
  { value: "60", label: "60 days" },
  { value: "90", label: "90 days" },
  { value: "365", label: "365 days" },
  { value: "never", label: "Never" },
] as const;
const expiryField = z.enum(EXPIRY_OPTIONS.map((option) => option.value));

// Access levels are the scope hierarchy presented as a single choice: each
// level grants itself and every level below it. Ordered low to high; `admin`
// is offered only to admins (the backend re-enforces the ceiling).
const ACCESS_LEVELS = [
  { value: "read", label: "Read-only", hint: "View library data. Cannot make changes." },
  { value: "write", label: "Read & write", hint: "View and modify library data." },
  { value: "admin", label: "Admin", hint: "Full access, including user management." },
] as const satisfies ReadonlyArray<{ value: Scope; label: string; hint: string }>;

function errorDetail(err: unknown): string {
  if (err instanceof ApiError) return err.detail;
  if (err instanceof Error) return err.message;
  return "Unknown error";
}

function validateName(value: string): string | null {
  const length = value.trim().length;
  return length === 0 || length > 255 ? "Enter a name (1-255 characters)." : null;
}

function formatDate(iso: string | null): string {
  if (iso === null) return "Never";
  return new Date(iso).toLocaleDateString();
}

function TokensPage(): ReactElement {
  const { data: me, isLoading: meLoading, isError: meError } = useAuthMe();
  const queryClient = useQueryClient();

  const {
    data: tokens,
    isLoading,
    error,
  } = useQuery({
    queryKey: queryKeys.tokens.list(),
    queryFn: ({ signal }) => listTokens(signal),
    enabled: me !== undefined,
  });

  const invalidateTokens = useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: queryKeys.tokens.all });
  }, [queryClient]);

  const revokeMutation = useMutation({
    mutationFn: (id: string) => revokeToken(id),
    onSuccess: () => {
      invalidateTokens();
      toast.success("Token revoked.");
    },
    onError: (err: Error) => {
      toast.error(`Revoke failed: ${errorDetail(err)}`);
    },
  });

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

  return (
    <div className="mx-auto max-w-4xl p-6">
      <div className="mb-6 flex items-center justify-between">
        <h1 className="text-2xl font-bold text-fg">API tokens</h1>
        <CreateTokenDialog isAdmin={me?.role === "admin"} onCreated={invalidateTokens} />
      </div>

      {isLoading && (
        <div className="space-y-2">
          {["name", "scopes", "actions"].map((slot) => (
            <Skeleton key={`skel-${slot}`} className="h-12 w-full" />
          ))}
        </div>
      )}

      {error && <p className="text-destructive">Failed to load tokens: {errorDetail(error)}</p>}

      {tokens && tokens.length === 0 && (
        <p className="py-8 text-center text-muted-foreground">No tokens found.</p>
      )}

      {tokens && tokens.length > 0 && (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead scope="col">Name</TableHead>
              <TableHead scope="col">Scopes</TableHead>
              <TableHead scope="col">Expires</TableHead>
              <TableHead scope="col">Last used</TableHead>
              <TableHead scope="col" className="text-right">
                Actions
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {tokens.map((token) => (
              <TokenRow
                key={token.id}
                token={token}
                onRevoke={(id) => {
                  revokeMutation.mutate(id);
                }}
                revoking={revokeMutation.isPending && revokeMutation.variables === token.id}
              />
            ))}
          </TableBody>
        </Table>
      )}
    </div>
  );
}

type TokenRowProps = {
  token: Token;
  onRevoke: (id: string) => void;
  revoking: boolean;
};

function TokenRow({ token, onRevoke, revoking }: Readonly<TokenRowProps>): ReactElement {
  return (
    <TableRow>
      <TableCell className="font-medium">{token.name}</TableCell>
      <TableCell>
        <div className="flex gap-1">
          {token.scopes.map((scope) => (
            <Badge key={scope} variant="outline">
              {scope}
            </Badge>
          ))}
        </div>
      </TableCell>
      <TableCell className="text-muted-foreground">{formatDate(token.expires_at)}</TableCell>
      <TableCell className="text-muted-foreground">{formatDate(token.last_used_at)}</TableCell>
      <TableCell className="text-right">
        <Button
          variant="destructive"
          size="sm"
          isDisabled={revoking}
          onPress={() => {
            onRevoke(token.id);
          }}
        >
          Revoke
        </Button>
      </TableCell>
    </TableRow>
  );
}

type CreateTokenDialogProps = {
  isAdmin: boolean;
  onCreated: () => void;
};

function CreateTokenDialog({ isAdmin, onCreated }: Readonly<CreateTokenDialogProps>): ReactElement {
  const [open, setOpen] = useState(false);
  const [level, setLevel] = useState<Scope>("write");

  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<CreateTokenResponse | null>(null);

  const mutation = useMutation({
    mutationFn: (input: { name: string; scopes: Scope[]; expiresInDays: number | null }) =>
      createToken(input),
    onSuccess: (token) => {
      onCreated();
      setCreated(token);
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
      setCreated(null);
      setLevel("write");
    }
  }

  async function submit(data: FormData): Promise<string | null> {
    if (mutation.isPending) return null;
    setError(null);
    const name = z.string().parse(data.get("name")).trim();
    if (validateName(name) !== null) return null;
    const expiry = expiryField.safeParse(data.get("expiry"));
    if (!expiry.success) return "Choose a valid expiry.";
    try {
      await mutation.mutateAsync({
        name,
        scopes: [level],
        expiresInDays: expiry.data === "never" ? null : Number(expiry.data),
      });
      return null;
    } catch (err) {
      return errorDetail(err);
    }
  }

  const availableLevels = ACCESS_LEVELS.filter((l) => l.value !== "admin" || isAdmin);

  return (
    <Dialog isOpen={open} onOpenChange={handleOpenChange}>
      <Button>New token</Button>
      <DialogContent>
        {created ? (
          <RevealCredential
            token={created}
            onDone={() => {
              handleOpenChange(false);
            }}
          />
        ) : (
          <>
            <DialogHeader>
              <DialogTitle>New token</DialogTitle>
              <DialogDescription>
                The credential is shown once after creation. Store it somewhere safe.
              </DialogDescription>
            </DialogHeader>
            <CreateTokenForm
              submit={submit}
              pending={mutation.isPending}
              serverError={error}
              level={level}
              setLevel={setLevel}
              availableLevels={availableLevels}
            />
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}

function CreateTokenForm({
  submit,
  pending,
  serverError,
  level,
  setLevel,
  availableLevels,
}: Readonly<{
  submit: (data: FormData) => Promise<string | null>;
  pending: boolean;
  serverError: string | null;
  level: Scope;
  setLevel: (level: Scope) => void;
  availableLevels: readonly (typeof ACCESS_LEVELS)[number][];
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
        name="name"
        type="text"
        isRequired
        validate={validateName}
        label="Name"
        defaultValue={state.values.name}
      />
      <Field>
        <Select
          className="flex flex-col gap-2"
          name="level"
          value={level}
          onChange={(v) => {
            if (typeof v !== "string") return;
            const match = ACCESS_LEVELS.find((l) => l.value === v);
            if (match) setLevel(match.value);
          }}
        >
          <AriaLabel className="flex w-fit gap-2 text-sm font-medium leading-snug">
            Access level
          </AriaLabel>
          <SelectTrigger id="create-token-level" className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {availableLevels.map((l) => (
              <SelectItem key={l.value} id={l.value}>
                {l.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <p className="text-muted-foreground text-xs">
          {ACCESS_LEVELS.find((l) => l.value === level)?.hint}
        </p>
      </Field>
      <Field>
        <Select
          className="flex flex-col gap-2"
          name="expiry"
          defaultValue={state.values.expiry ?? "90"}
        >
          <AriaLabel className="flex w-fit gap-2 text-sm font-medium leading-snug">
            Expires
          </AriaLabel>
          <SelectTrigger id="create-token-expiry" className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {EXPIRY_OPTIONS.map((opt) => (
              <SelectItem key={opt.value} id={opt.value}>
                {opt.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>
      {error ? <FieldError>{error}</FieldError> : null}
      <DialogFooter>
        <DialogClose type="button">Cancel</DialogClose>
        <Button type="submit" isPending={pending || actionPending}>
          Create
        </Button>
      </DialogFooter>
    </Form>
  );
}

type RevealCredentialProps = {
  token: CreateTokenResponse;
  onDone: () => void;
};

/**
 * Splits the Bearer credential on its last `.` into the OPDS Basic-auth
 * username/password pair, mirroring `backend/src/routes/tokens.rs`'s
 * documented format. Returns `null` when the credential has no `.`
 * separator (an unexpected shape), so the caller can omit the OPDS split
 * rather than render a credential silently missing a character.
 */
function splitCredential(bearer: string): { username: string; password: string } | null {
  const i = bearer.lastIndexOf(".");
  if (i <= 0 || i >= bearer.length - 1) return null;
  return { username: bearer.slice(0, i), password: bearer.slice(i + 1) };
}

async function copyToClipboard(value: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(value);
    toast.success("Copied.");
  } catch {
    toast.error("Copy failed. Select the text and copy it manually.");
  }
}

function RevealCredential({ token, onDone }: Readonly<RevealCredentialProps>): ReactElement {
  const basicAuth = splitCredential(token.token);

  return (
    <>
      <DialogHeader>
        <DialogTitle>Token created</DialogTitle>
        <DialogDescription>
          This is the only time the credential is shown. Copy it now.
        </DialogDescription>
      </DialogHeader>
      <div className="flex flex-col gap-4">
        <CopyField label="Bearer token" value={token.token} />
        {basicAuth && (
          <div className="border-border rounded-md border p-3">
            <p className="text-fg-muted mb-2 text-xs">
              For OPDS / e-reader apps that prompt for username and password separately:
            </p>
            <CopyField label="Username" value={basicAuth.username} />
            <CopyField label="Password" value={basicAuth.password} />
          </div>
        )}
      </div>
      <DialogFooter>
        <Button type="button" onPress={onDone}>
          Done
        </Button>
      </DialogFooter>
    </>
  );
}

type CopyFieldProps = {
  label: string;
  value: string;
};

function CopyField({ label, value }: Readonly<CopyFieldProps>): ReactElement {
  return (
    <TextField isReadOnly value={value} render={(props) => <Field {...props} />}>
      <AriaLabel render={(props) => <FieldLabel {...props} />}>{label}</AriaLabel>
      <div className="flex items-center gap-2">
        <AriaInput render={(props) => <Input {...props} />} className="font-mono text-xs" />
        <Button
          type="button"
          variant="outline"
          size="icon"
          aria-label={`Copy ${label}`}
          onPress={() => {
            void copyToClipboard(value);
          }}
        >
          <Copy className="size-4" aria-hidden="true" />
        </Button>
      </div>
    </TextField>
  );
}

export { TokensPage };
