import type { ReactElement, ReactNode } from "react";
import { ThemeSwitcher } from "@/components/theme-switcher";
import { Lockup } from "@/components/Lockup";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Checkbox } from "@/components/ui/checkbox";
import { Badge } from "@/components/ui/badge";
import {
  RadioGroup,
  RadioGroupItem,
} from "@/components/ui/radio-group";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { useTheme } from "@/lib/theme/ThemeProvider";

interface SectionProps {
  title: string;
  description?: string;
  children: ReactNode;
}

function Section({ title, description, children }: SectionProps): ReactElement {
  return (
    <section className="border-b border-border last:border-b-0 py-8">
      <header className="mb-4">
        <h2 className="text-fg font-display text-xl font-semibold">{title}</h2>
        {description ? (
          <p className="text-fg-muted text-sm mt-1">{description}</p>
        ) : null}
      </header>
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">{children}</div>
    </section>
  );
}

interface ExampleProps {
  label: string;
  children: ReactNode;
}

function Example({ label, children }: ExampleProps): ReactElement {
  return (
    <div className="bg-surface border border-border rounded-md p-4 flex flex-col gap-3">
      <span className="text-fg-muted text-xs uppercase tracking-wider">
        {label}
      </span>
      {children}
    </div>
  );
}

export default function DesignSystemPage(): ReactElement {
  const { effective } = useTheme();
  return (
    <main className="bg-canvas text-fg min-h-screen">
      <header className="bg-canvas-2 border-b border-border px-6 py-4 flex items-center justify-between sticky top-0 z-10">
        <div className="flex items-center gap-3">
          <Lockup
            size={28}
            theme={effective === "dark" ? "dark" : "light"}
          />
          <span className="text-fg-muted text-sm">/ design-system</span>
        </div>
        <ThemeSwitcher />
      </header>
      <div className="max-w-6xl mx-auto px-6">
        <Section
          title="Typography"
          description="Display = Author Variable; Body = Satoshi Variable; Mono = JetBrains Mono (loaded but conditional, UNK-113)."
        >
          <Example label="Display">
            <p className="font-display text-3xl">A library worth keeping</p>
          </Example>
          <Example label="Body">
            <p className="font-body text-base text-fg">
              The Reverie design system codifies brand identity into utility
              tokens. Every surface inherits the canonical palette.
            </p>
          </Example>
          <Example label="Mono (conditional)">
            <code className="font-mono text-sm text-fg-muted">
              const accent = &quot;#C9A961&quot;;
            </code>
          </Example>
        </Section>

        <Section
          title="Buttons"
          description="Brand rule: bg-accent (Reverie Gold) is reserved for large CTAs, focus rings, and recovery actions on Light theme — the 8E6F38 darkened gold passes AA at 18pt+ but not at 14pt body. Default-size actions use the outline variant; lg size unlocks the gold fill."
        >
          <Example label="Primary action (large CTA)">
            <Button size="lg">Add to library</Button>
          </Example>
          <Example label="Outline (default size)">
            <Button variant="outline">Browse</Button>
          </Example>
          <Example label="Ghost">
            <Button variant="ghost">Cancel</Button>
          </Example>
          <Example label="Disabled">
            <Button disabled variant="outline">
              Pending
            </Button>
          </Example>
          <Example label="Sizes (outline, brand-aligned)">
            <div className="flex flex-col items-start gap-2">
              <Button size="sm" variant="outline">
                Small
              </Button>
              <Button size="default" variant="outline">
                Default
              </Button>
              <Button size="lg">Large</Button>
            </div>
          </Example>
          <Example label="Loading (opacity-pulse)">
            <Button
              size="lg"
              className="animate-[loading-pulse_1.6s_ease-in-out_infinite]"
            >
              Saving…
            </Button>
          </Example>
        </Section>

        <Section title="Form primitives">
          <Example label="Input + Label">
            <div className="flex flex-col gap-1.5 w-full">
              <Label htmlFor="ex-input">Title</Label>
              <Input id="ex-input" placeholder="The Brothers Karamazov" />
            </div>
          </Example>
          <Example label="Textarea">
            <Textarea placeholder="Notes…" rows={4} />
          </Example>
          <Example label="Switch">
            <div className="flex items-center gap-2">
              <Switch id="ex-switch" />
              <Label htmlFor="ex-switch">Enable OPDS feed</Label>
            </div>
          </Example>
          <Example label="Checkbox">
            <div className="flex items-center gap-2">
              <Checkbox id="ex-check" />
              <Label htmlFor="ex-check">Mark as read</Label>
            </div>
          </Example>
          <Example label="Radio group">
            <RadioGroup defaultValue="epub" className="flex flex-col gap-2">
              <div className="flex items-center gap-2">
                <RadioGroupItem id="ex-r-epub" value="epub" />
                <Label htmlFor="ex-r-epub">EPUB</Label>
              </div>
              <div className="flex items-center gap-2">
                <RadioGroupItem id="ex-r-pdf" value="pdf" />
                <Label htmlFor="ex-r-pdf">PDF</Label>
              </div>
            </RadioGroup>
          </Example>
          <Example label="Select">
            <Select defaultValue="title">
              <SelectTrigger className="w-full" aria-label="Sort order">
                <SelectValue placeholder="Sort by" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="title">Title</SelectItem>
                <SelectItem value="author">Author</SelectItem>
                <SelectItem value="date">Date added</SelectItem>
              </SelectContent>
            </Select>
          </Example>
        </Section>

        <Section title="Surfaces and indicators">
          <Example label="Card">
            <Card>
              <CardHeader>
                <CardTitle>Crime and Punishment</CardTitle>
                <CardDescription>Fyodor Dostoevsky · 1866</CardDescription>
              </CardHeader>
              <CardContent>
                <p className="text-fg-muted text-sm">
                  A novel about guilt, morality, and the boundaries of
                  conscience.
                </p>
              </CardContent>
              <CardFooter>
                <Button size="sm" variant="outline">
                  Open
                </Button>
              </CardFooter>
            </Card>
          </Example>
          <Example label="Badges">
            <div className="flex flex-wrap items-center gap-2">
              <Badge>Default</Badge>
              <Badge variant="outline">Outline</Badge>
              <Badge variant="secondary">Secondary</Badge>
            </div>
          </Example>
          <Example label="Skeleton (loading)">
            <div className="flex flex-col gap-2 w-full">
              <Skeleton className="h-4 w-3/4" />
              <Skeleton className="h-4 w-2/3" />
              <Skeleton className="h-4 w-1/2" />
            </div>
          </Example>
          <Example label="Separator">
            <div className="flex flex-col w-full">
              <span className="text-fg-muted text-sm">above</span>
              <Separator className="my-2" />
              <span className="text-fg-muted text-sm">below</span>
            </div>
          </Example>
        </Section>

        <Section
          title="Tokens"
          description="The canonical palette renders here directly in the active theme."
        >
          {[
            "canvas",
            "canvas-2",
            "surface",
            "surface-2",
            "border",
            "border-strong",
            "fg",
            "fg-muted",
            "fg-faint",
            "accent",
            "accent-soft",
            "accent-strong",
            "fg-on-accent",
          ].map((token) => (
            <Example key={token} label={`--color-${token}`}>
              <div
                className={`bg-${token} h-12 w-full border border-border rounded-sm`}
              />
            </Example>
          ))}
        </Section>

        <Section
          title="State expression (no hue)"
          description="State communicates through typography weight, opacity, motion, and the gold accent — never a state-color hue."
        >
          <Example label="Error pattern (font-semibold + gold recovery)">
            <div className="flex flex-col gap-2">
              <p className="text-fg font-semibold">
                Could not save the shelf.
              </p>
              <Button variant="outline" size="sm" className="self-start">
                Retry
              </Button>
            </div>
          </Example>
          <Example label="Disabled (opacity-50 + text-fg-muted, aria-disabled)">
            <p
              className="opacity-50 text-fg-muted"
              aria-disabled="true"
            >
              Unavailable in offline mode
            </p>
          </Example>
          <Example label="Selected (bg-accent-soft + text-fg)">
            <span className="bg-accent-soft text-fg rounded-sm px-2 py-1 text-sm">
              Currently reading
            </span>
          </Example>
        </Section>
      </div>
    </main>
  );
}
