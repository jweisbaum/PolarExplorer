/**
 * A Blend entry for tests' project summaries: the defaults a new project
 * starts with (spec.md 8, 12.2), nothing blended yet.
 */
import type { BlendSummary } from "./generated/BlendSummary";

export const TEST_BLEND: BlendSummary = {
  colour: "#e0457b",
  visible: true,
  direct: 0,
  filled: 0,
  empty: 180,
  twa: [0, 30, 35, 40, 45, 52, 60, 70, 75, 80, 90, 100, 110, 120, 135, 150, 160, 170, 180],
  tws: [4, 6, 8, 10, 12, 14, 16, 20, 25, 30],
  min_samples: 5,
  n_full: 30,
  smoothing: false,
  default_statistic: "p90", polar_statistic: "mean",
  global_filters: null,
  wave_ranges: { hs: { min: null, max: null }, waveAngle: { min: null, max: null }, wavePeriod: { min: null, max: null } },
  asymmetric: false,
  interpolation: "linear",
  correction_count: 0,
  priority_groups: [],
  priority_min_samples: 5,
};
