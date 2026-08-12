import type { ReactElement, ReactNode } from "react";

import { Lockup } from "@/components/Lockup";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { useTheme } from "@/lib/theme/ThemeProvider";

type AuthShellProps = {
  title: string;
  description?: string;
  children: ReactNode;
  footer?: ReactNode;
};

/**
 * Full-page chrome shared by the pre-auth screens (login / setup /
 * forgot-password). These routes mount as siblings of the app-shell
 * root, so they render without the rail / utility strip and without the
 * `useSessionRecovery` redirect funnel. The brand `Lockup` styles its
 * wordmark inline, so the wordmark shows even before the theme CSS
 * resolves; its tint follows the effective theme. The glyph beside it is
 * fetched from the shipped brand assets and can be absent on a surface
 * that paints without them.
 */
export function AuthShell({ title, description, children, footer }: AuthShellProps): ReactElement {
  const { effective } = useTheme();
  return (
    <main className="bg-canvas text-fg flex min-h-screen items-center justify-center px-6 py-16">
      <div className="w-full max-w-sm">
        <div className="mb-8 flex justify-center">
          <Lockup size={32} theme={effective} />
        </div>
        <Card>
          <CardHeader>
            <CardTitle className="text-lg">{title}</CardTitle>
            {description !== undefined ? <CardDescription>{description}</CardDescription> : null}
          </CardHeader>
          <CardContent>{children}</CardContent>
        </Card>
        {footer !== undefined ? (
          <div className="text-fg-muted mt-4 text-center text-sm">{footer}</div>
        ) : null}
      </div>
    </main>
  );
}
