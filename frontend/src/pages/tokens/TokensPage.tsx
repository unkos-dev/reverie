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
import { type ReactElement, type SyntheticEvent, useCallback, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Copy } from "lucide-react";
import { toast } from "sonner";

import { useAuthMe } from "@/hooks/useAuthMe";
import { queryKeys } from "@/lib/query/keys";
import { listTokens, createToken, revokeToken } from "@/api/tokens";
import type { Token, Scope, CreateTokenResponse } from "@/api/tokens";
import { ApiError } from "@/api";
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
type ExpiryValue = (typeof EXPIRY_OPTIONS)[number]["value"];

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
          {Array.from({ length: 3 }, (_, i) => (
            <Skeleton key={`skel-${String(i)}`} className="h-12 w-full" />
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
              <TableHead>Name</TableHead>
              <TableHead>Scopes</TableHead>
              <TableHead>Expires</TableHead>
              <TableHead>Last used</TableHead>
              <TableHead className="text-right">Actions</TableHead>
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
          disabled={revoking}
          onClick={() => {
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
  const [expiry, setExpiry] = useState<ExpiryValue>("90");
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
      setExpiry("90");
    }
  }

  function handleSubmit(e: SyntheticEvent<HTMLFormElement>): void {
    e.preventDefault();
    setError(null);
    const data = new FormData(e.currentTarget);
    const name = formString(data, "name").trim();
    if (name.length === 0 || name.length > 255) {
      setError("Enter a name (1-255 characters).");
      return;
    }
    mutation.mutate({
      name,
      scopes: [level],
      expiresInDays: expiry === "never" ? null : Number(expiry),
    });
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogTrigger asChild>
        <Button>New token</Button>
      </DialogTrigger>
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
            <form onSubmit={handleSubmit} className="flex flex-col gap-4" noValidate>
              <Field>
                <FieldLabel htmlFor="create-token-name">Name</FieldLabel>
                <Input id="create-token-name" name="name" type="text" required />
              </Field>
              <Field>
                <FieldLabel htmlFor="create-token-level">Access level</FieldLabel>
                <Select
                  value={level}
                  onValueChange={(v) => {
                    const match = ACCESS_LEVELS.find((l) => l.value === v);
                    if (match) setLevel(match.value);
                  }}
                >
                  <SelectTrigger id="create-token-level" className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {ACCESS_LEVELS.filter((l) => l.value !== "admin" || isAdmin).map((l) => (
                      <SelectItem key={l.value} value={l.value}>
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
                <FieldLabel htmlFor="create-token-expiry">Expires</FieldLabel>
                <Select
                  value={expiry}
                  onValueChange={(v) => {
                    const match = EXPIRY_OPTIONS.find((o) => o.value === v);
                    if (match) setExpiry(match.value);
                  }}
                >
                  <SelectTrigger id="create-token-expiry" className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {EXPIRY_OPTIONS.map((opt) => (
                      <SelectItem key={opt.value} value={opt.value}>
                        {opt.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Field>
              {error ? <FieldError>{error}</FieldError> : null}
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
          </>
        )}
      </DialogContent>
    </Dialog>
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
        <Button type="button" onClick={onDone}>
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
    <Field>
      <FieldLabel>{label}</FieldLabel>
      <div className="flex items-center gap-2">
        <Input readOnly value={value} className="font-mono text-xs" />
        <Button
          type="button"
          variant="outline"
          size="icon"
          aria-label={`Copy ${label}`}
          onClick={() => {
            void copyToClipboard(value);
          }}
        >
          <Copy className="size-4" aria-hidden="true" />
        </Button>
      </div>
    </Field>
  );
}

export { TokensPage };
