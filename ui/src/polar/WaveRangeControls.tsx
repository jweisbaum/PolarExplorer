import { useId, useMemo, useRef, useState, type PointerEvent } from "react";
import { msg, useT } from "../i18n";
import type { Units } from "../generated/Units";
import { waveUnit } from "../panels/filterUnits";
import type { ScenePacket } from "./scenePacket";
import { NO_WAVE_RANGES, waveRangesActive, type WaveMetric, type WaveRanges } from "./waveRanges";

const controls = [
  { key: "hs", label: msg("Wave height"), lower: "wave-height-min", upper: "wave-height-max", move: "wave-height-move", moveLabel: msg("Move wave height range"), lowerLabel: msg("Minimum wave height shown"), upperLabel: msg("Maximum wave height shown") },
  { key: "waveAngle", label: msg("Wave angle off the bow"), lower: "wave-angle-min", upper: "wave-angle-max", move: "wave-angle-move", moveLabel: msg("Move wave angle range"), lowerLabel: msg("Minimum wave angle shown"), upperLabel: msg("Maximum wave angle shown") },
  { key: "wavePeriod", label: msg("Wave period"), lower: "wave-period-min", upper: "wave-period-max", move: "wave-period-move", moveLabel: msg("Move wave period range"), lowerLabel: msg("Minimum wave period shown"), upperLabel: msg("Maximum wave period shown") },
] as const;

function RangeSelection({ min, max, ceiling, step, feature, label, onMove, onDrag, namespace }: {
  min: number; max: number; ceiling: number; step: number; feature: string; label: string; namespace: "view3d" | "plot";
  onMove: (min: number, max: number) => void;
  onDrag: (active: boolean) => void;
}) {
  const t = useT();
  const drag = useRef<{ pointer: number; x: number; width: number; min: number; max: number; ceiling: number; step: number } | null>(null);
  const move = (start: { min: number; max: number; ceiling: number; step: number }, distance: number) => {
    const delta = Math.max(-start.min, Math.min(start.ceiling - start.max, Math.round(distance / start.step) * start.step));
    const lower = Math.max(0, start.min + delta);
    const upper = Math.min(start.ceiling, start.max + delta);
    if (lower !== min || upper !== max) onMove(lower, upper);
  };
  const pointerMove = (event: PointerEvent<HTMLButtonElement>) => {
    const start = drag.current;
    if (start?.pointer === event.pointerId) move(start, (event.clientX - start.x) / start.width * start.ceiling);
  };
  const finish = (event: PointerEvent<HTMLButtonElement>) => {
    if (drag.current?.pointer !== event.pointerId) return;
    drag.current = null;
    onDrag(false);
    if (event.currentTarget.hasPointerCapture?.(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  };
  return <button type="button" className="wave-range-selection" data-feature={`${namespace}:${feature}`}
    aria-label={label} title={t("Drag to move both limits. Arrow keys move the range; Home and End move it to either end.")}
    style={{ left: `calc(${min / ceiling * 100}% + 7px)`, width: `max(0px, calc(${(max - min) / ceiling * 100}% - 14px))` }}
    onPointerDown={event => {
      if (event.button !== 0 || drag.current) return;
      const width = event.currentTarget.parentElement!.getBoundingClientRect().width;
      if (width <= 0) return;
      event.preventDefault();
      event.currentTarget.focus({ preventScroll: true });
      event.currentTarget.setPointerCapture(event.pointerId);
      drag.current = { pointer: event.pointerId, x: event.clientX, width, min, max, ceiling, step };
      onDrag(true);
    }}
    onPointerMove={pointerMove}
    onPointerUp={event => { pointerMove(event); finish(event); }}
    onPointerCancel={finish} onLostPointerCapture={finish}
    onKeyDown={event => {
      const distance = event.key === "ArrowLeft" || event.key === "ArrowDown" ? -step
        : event.key === "ArrowRight" || event.key === "ArrowUp" ? step
        : event.key === "Home" ? -ceiling : event.key === "End" ? ceiling : null;
      if (distance === null) return;
      event.preventDefault();
      move({ min, max, ceiling, step }, distance);
    }} />;
}

export default function WaveRangeControls({ samples, ranges, onChange, units, count, maxima: suppliedMaxima,
  namespace = "view3d", disabled = false }: {
  samples?: ScenePacket["samples"];
  maxima?: readonly (number | null)[];
  ranges: WaveRanges; onChange: (value: WaveRanges) => void; units: Units; count: number;
  namespace?: "view3d" | "plot"; disabled?: boolean;
}) {
  const t = useT();
  const id = useId();
  // A saved bound may exceed the remaining samples after removing a track.
  // Keep its scale fixed during a drag, even while that bound moves inward.
  const [dragCeilings, setDragCeilings] = useState<Partial<Record<WaveMetric, number>>>({});
  const height = waveUnit(units);
  const sampledMaxima = useMemo(() => controls.map(({ key }) => {
    let max: number | null = null;
    for (const v of samples?.[key] ?? []) if (Number.isFinite(v)) max = Math.max(max ?? 0, v);
    return max;
  }), [samples?.hs, samples?.waveAngle, samples?.wavePeriod]);
  const maxima = suppliedMaxima ?? sampledMaxima;
  const change = (key: WaveMetric, end: "min" | "max", value: number, factor: number, ceiling: number) => {
    const old = ranges[key];
    const stored = value / factor;
    const bound = end === "min" ? Math.min(stored, old.max ?? Infinity) : Math.max(stored, old.min ?? 0);
    const next = (end === "min" && bound <= 0) || (end === "max" && bound >= ceiling / factor) ? null : bound;
    onChange({ ...ranges, [key]: { ...old, [end]: next } });
  };
  return <section className="wave-display-ranges" aria-label={t("Wave range filters")}>
    <header className="wave-range-header">
      <h3 title={t("Filter track samples and update the blend in both views. These ranges apply in addition to other filters.")}>{t("Wave range filters")}</h3>
      <div className="wave-range-footer">
        <span role="status">{t("{count} samples shown", { count })}</span>
        <button data-feature={`${namespace}:wave-ranges-reset`} disabled={disabled || !waveRangesActive(ranges)} onClick={() => onChange(NO_WAVE_RANGES)}>{t("Reset ranges")}</button>
      </div>
    </header>
    {controls.map((control, i) => {
      const { key } = control;
      const factor = key === "hs" ? height.factor : 1;
      const symbol = key === "hs" ? height.symbol : key === "waveAngle" ? "°" : "s";
      const step = key === "waveAngle" ? 1 : 0.1;
      const r = ranges[key];
      const ceiling = dragCeilings[key] ?? (key === "waveAngle" ? 180 : Math.max(1, Math.ceil(Math.max(maxima[i] ?? 0, r.min ?? 0, r.max ?? 0) * factor * 10) / 10));
      const missing = maxima[i] === null;
      const values = { min: (r.min ?? 0) * factor, max: r.max === null ? ceiling : r.max * factor };
      // Divide pointer hit areas halfway between the handles. Both native
      // sliders keep their full scale and keyboard behaviour; even coincident
      // handles remain reachable on their respective halves of the bullet.
      const split = `calc(7px + (100% - 14px) * ${(values.min + values.max) / (2 * ceiling)})`;
      const display = (end: "min" | "max") => values[end].toFixed(key === "waveAngle" ? 0 : 1);
      return <fieldset key={key} disabled={disabled || missing} aria-labelledby={`${id}-${key}-label`}>
        <div className="wave-range-row">
          <span id={`${id}-${key}-label`} className="wave-range-label">
            {t(control.label)} <span className="muted">({symbol})</span>
            {missing && <span className="muted wave-range-missing">{t("No wave data")}</span>}
          </span>
          <output htmlFor={`${id}-${key}-min`} title={t("Lower")}>{display("min")}</output>
          <div className="wave-range-slider">
            <div className="wave-range-track">
              <span aria-hidden="true" style={{ left: `${values.min / ceiling * 100}%`, width: `${(values.max - values.min) / ceiling * 100}%` }} />
              <RangeSelection {...values} ceiling={ceiling} step={step} namespace={namespace} feature={control.move} label={t(control.moveLabel)}
                onDrag={active => setDragCeilings(previous => ({ ...previous, [key]: active ? ceiling : undefined }))}
                onMove={(min, max) => onChange({ ...ranges, [key]: {
                  min: min <= 0 ? null : min / factor, max: max >= ceiling ? null : max / factor,
                } })} />
            </div>
            {(["min", "max"] as const).map(end => <input key={end} id={`${id}-${key}-${end}`}
              type="range" min={0} max={ceiling} step={step} value={values[end]}
              style={{ clipPath: end === "min" ? `inset(0 calc(100% - ${split}) 0 0)` : `inset(0 0 0 ${split})` }}
              data-feature={`${namespace}:${end === "min" ? control.lower : control.upper}`}
              aria-label={t(end === "min" ? control.lowerLabel : control.upperLabel)}
              aria-valuemin={end === "min" ? 0 : values.min}
              aria-valuemax={end === "min" ? values.max : ceiling}
              aria-valuetext={`${display(end)} ${symbol}`}
              onChange={e => change(key, end, Number(e.target.value), factor, ceiling)} />)}
          </div>
          <output htmlFor={`${id}-${key}-max`} title={t("Upper")}>{display("max")}</output>
        </div>
      </fieldset>;
    })}
  </section>;
}
