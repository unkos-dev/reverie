import { useMutation, useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState, type ReactElement } from "react";
import { Link } from "react-router";

import { ApiError } from "@/api";
import {
  fetchEmailVerification,
  requestEmailVerification,
  type EmailVerification,
} from "@/api/auth";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { FieldError } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/skeleton";
import { queryKeys } from "@/lib/query/keys";
import { AuthShell } from "./auth-shell";

type EmailView = "loading" | "no-address" | "verified" | "unavailable" | "sent" | "unverified";

function emailView(status: EmailVerification | undefined, sent: boolean): EmailView {
  if (status === undefined) return "loading";
  if (status.email === null) return "no-address";
  if (status.email_verified) return "verified";
  if (!status.mail_enabled) return "unavailable";
  return sent ? "sent" : "unverified";
}

const emailBody = {
  "no-address": "Your account has no email address. Ask your administrator to add one.",
  verified: "Your email address is verified.",
  unavailable: "Email isn't set up on this server, so this address can't be verified yet.",
  sent: "Verification email requested. Open the link in it while signed in to this account. The email says when the link expires.",
  unverified: "Verify that you control this email address.",
};

function requestError(cause: unknown): string {
  if (cause instanceof ApiError) {
    if (cause.status === 503) return "Email isn't available right now. Try again later.";
    if (cause.status === 429) return cause.detail;
  }
  return "Could not confirm the request. Try again.";
}

export function Component(): ReactElement {
  const status = useQuery({
    queryKey: queryKeys.auth.emailVerification(),
    queryFn: ({ signal }) => fetchEmailVerification(signal),
    retry: false,
  });
  const [sent, setSent] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const title = useRef<HTMLHeadingElement>(null);
  const view = emailView(status.data, sent);
  useEffect(() => {
    if (view !== "loading") title.current?.focus();
  }, [view]);
  const request = useMutation({
    mutationFn: () => requestEmailVerification(),
    onSuccess: () => {
      setSent(true);
      setError(null);
    },
    onError: (cause) => {
      setSent(false);
      setError(requestError(cause));
    },
  });
  const footer = (
    <Link to="/library" className="hover:text-fg underline underline-offset-4">
      Back to library
    </Link>
  );
  if (status.data === undefined)
    return (
      <AuthShell title="" footer={footer}>
        {status.isError ? (
          <>
            <FieldError>Could not load your email settings.</FieldError>
            <Button
              className="mt-4 w-full"
              isDisabled={status.isFetching}
              onPress={() => {
                void status.refetch();
              }}
            >
              Try again
            </Button>
          </>
        ) : (
          <div role="status" aria-label="Loading email settings">
            <Skeleton className="h-6 w-40" />
          </div>
        )}
      </AuthShell>
    );
  return (
    <AuthShell
      title={
        <h1 ref={title} tabIndex={-1}>
          Email address
        </h1>
      }
      description={emailBody[view === "loading" ? "unverified" : view]}
      footer={footer}
    >
      <div className="flex flex-col gap-4">
        {status.data.email !== null ? (
          <div className="flex flex-wrap items-center gap-2">
            <span className="break-all">{status.data.email}</span>
            <Badge variant="outline">
              {status.data.email_verified ? "Verified" : "Not verified"}
            </Badge>
          </div>
        ) : null}
        {error !== null ? <FieldError>{error}</FieldError> : null}
        {view === "sent" || view === "unverified" ? (
          <Button
            className="w-full"
            variant={sent ? "outline" : "default"}
            isDisabled={request.isPending}
            onPress={() => {
              request.mutate();
            }}
          >
            {sent ? "Send again" : "Send verification email"}
          </Button>
        ) : null}
      </div>
    </AuthShell>
  );
}
