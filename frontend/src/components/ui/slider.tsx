import type { ReactElement } from "react";
import {
  Label,
  Slider as AriaSlider,
  SliderFill,
  SliderOutput,
  SliderThumb,
  SliderTrack,
  type SliderProps,
} from "react-aria-components";

export function Slider({ label, ...props }: SliderProps<number> & { label: string }): ReactElement {
  return (
    <AriaSlider {...props} className="grid w-40 shrink-0 grid-cols-2 items-center gap-x-2 text-xs">
      <Label className="text-fg-muted">{label}</Label>
      <SliderOutput className="text-fg-muted text-right tabular-nums">
        {({ state }) => `${String(state.getThumbValue(0))} px`}
      </SliderOutput>
      <SliderTrack className="relative col-span-2 h-5 w-full">
        <div className="bg-border absolute top-1/2 h-1 w-full -translate-y-1/2 rounded-full">
          <SliderFill className="bg-accent h-full rounded-full" />
        </div>
        <SliderThumb
          aria-label={`${label} in pixels`}
          className="border-accent-text bg-accent top-1/2 size-3.5 rounded-full border outline-none data-focus-visible:outline-2 data-focus-visible:outline-offset-2 data-focus-visible:outline-(--focus-ring) data-dragging:bg-accent-strong"
        />
      </SliderTrack>
    </AriaSlider>
  );
}
