/**
 * `/tokens` — self-service device-token management.
 *
 * Lists the caller's own active tokens with revoke, and mints new ones
 * through a scope + expiry dialog. Reachable by any authenticated user;
 * the `admin` scope option only renders for `me.role === 'admin'` — the
 * backend re-enforces this ceiling regardless of what the dialog offers.
 */
import { type ReactElement, type SyntheticEvent, useCallback, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Copy } from "lucide-react";
import { toast } from "sonner";

import { useAuthMe } from "@/hooks/useAuthMe";
import { queryKeys } from "@/lib/query/keys";
import { listTokens, createToken, revokeToken } from "@/api/tokens";
import type { Token, Scope, CreateTokenResponse } from "@/api/tokens";
import { SCOPE_VALUES } from "@/api/tokens";
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
import { Checkbox } from "@/components/ui/checkbox";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
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
  const [scopes, setScopes] = useState<Set<Scope>>(new Set(["read", "write"]));
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
      setScopes(new Set(["read", "write"]));
      setExpiry("90");
    }
  }

  function toggleScope(scope: Scope, checked: boolean): void {
    setScopes((prev) => {
      const next = new Set(prev);
      if (checked) {
        next.add(scope);
      } else {
        next.delete(scope);
      }
      return next;
    });
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
    if (scopes.size === 0) {
      setError("Select at least one scope.");
      return;
    }
    mutation.mutate({
      name,
      scopes: Array.from(scopes),
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
                <FieldLabel>Scopes</FieldLabel>
                <div className="flex flex-col gap-2">
                  {SCOPE_VALUES.filter((scope) => scope !== "admin" || isAdmin).map((scope) => (
                    <div key={scope} className="flex items-center gap-2">
                      <Checkbox
                        id={`create-token-scope-${scope}`}
                        checked={scopes.has(scope)}
                        onCheckedChange={(checked) => {
                          toggleScope(scope, checked === true);
                        }}
                      />
                      <Label htmlFor={`create-token-scope-${scope}`}>{scope}</Label>
                    </div>
                  ))}
                </div>
              </Field>
              <Field>
                <FieldLabel htmlFor="create-token-expiry">Expires</FieldLabel>
                <Select
                  value={expiry}
                  onValueChange={(v: ExpiryValue) => {
                    setExpiry(v);
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
 * documented format.
 */
function splitCredential(bearer: string): { username: string; password: string } {
  const i = bearer.lastIndexOf(".");
  return { username: bearer.slice(0, i), password: bearer.slice(i + 1) };
}

function copyToClipboard(value: string): void {
  void navigator.clipboard.writeText(value);
  toast.success("Copied.");
}

function RevealCredential({ token, onDone }: Readonly<RevealCredentialProps>): ReactElement {
  const { username, password } = splitCredential(token.token);

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
        <div className="border-border rounded-md border p-3">
          <p className="text-fg-muted mb-2 text-xs">
            For OPDS / e-reader apps that prompt for username and password separately:
          </p>
          <CopyField label="Username" value={username} />
          <CopyField label="Password" value={password} />
        </div>
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
            copyToClipboard(value);
          }}
        >
          <Copy className="size-4" aria-hidden="true" />
        </Button>
      </div>
    </Field>
  );
}

export { TokensPage };
