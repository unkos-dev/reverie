import { Search, X } from "lucide-react";
import type { ReactElement } from "react";
import { SearchField, Input, FieldError } from "react-aria-components";
import { Button } from "@/components/ui/button";
import { libraryTextError } from "@/routes/library-params";

type QuickSearchProps = { value: string; onChange: (value: string) => void };

export function QuickSearch({ value, onChange }: Readonly<QuickSearchProps>): ReactElement {
  const error = libraryTextError(value);
  return (
    <SearchField
      aria-label="Search your library"
      value={value}
      onChange={onChange}
      isInvalid={error !== undefined}
      className="min-w-0 flex-1"
    >
      <div className="relative">
        <Search
          aria-hidden="true"
          className="text-fg-muted pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2"
        />
        <Input
          placeholder="Search your library"
          className="border-input text-fg placeholder:text-fg-muted dark:bg-input/30 h-9 w-full min-w-0 rounded-lg border bg-transparent pl-9 pr-9 text-sm outline-none"
        />
        <Button
          aria-label="Clear search"
          variant="ghost"
          size="icon"
          className="absolute right-0.5 top-0.5 data-[disabled]:hidden"
        >
          <X aria-hidden="true" />
        </Button>
      </div>
      <FieldError className="text-fg-muted mt-1 text-sm">{error}</FieldError>
    </SearchField>
  );
}
