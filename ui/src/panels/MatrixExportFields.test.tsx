// @vitest-environment happy-dom
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";
import MatrixExportFields, { defaultMatrixExport, matrixExportError } from "./MatrixExportFields";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("accepts only fixed direction counts and float32-representable cutoffs", () => {
  for (const count of [2, 4, 8, 20]) {
    const options = defaultMatrixExport();
    options.waves!.angle.count = count;
    expect(matrixExportError(options, 1)).toBeNull();
  }
  for (const count of [0, 1, 3, 16, 18, 36, NaN]) {
    const options = defaultMatrixExport();
    options.waves!.angle.count = count;
    expect(matrixExportError(options, 1)).not.toBeNull();
  }
  for (const cutoffs of [[1, 1 + 1e-9], [1e-100], [100 - 1e-9]]) {
    const options = defaultMatrixExport();
    options.waves!.height.cutoffs = cutoffs;
    expect(matrixExportError(options, 1)).not.toBeNull();
  }
  expect(matrixExportError({ ...defaultMatrixExport(), waves: null }, 1)).toBeNull();
});

it("selects fixed directions, shows their edges and disables all waves together", async () => {
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  let latest = defaultMatrixExport();
  function Controls() {
    const [value, setValue] = useState(latest);
    return <MatrixExportFields value={value} disabled={false} onChange={next => { latest = next; setValue(next); }} />;
  }
  try {
    await act(async () => root.render(<Controls />));
    const select = host.querySelector<HTMLSelectElement>('[data-feature="matrix-export:directions"]')!;
    expect(Array.from(select.options, o => o.value)).toEqual(["2", "4", "8", "20"]);
    expect(host.textContent).toContain("-45–<45");
    await act(async () => { select.value = "20"; select.dispatchEvent(new Event("change", { bubbles: true })); });
    expect(latest.waves!.angle.count).toBe(20);
    expect(host.textContent).toContain("-9–<9");
    expect(host.textContent).toContain("Wave combinations: 160");
    await act(async () => host.querySelector<HTMLInputElement>('[data-feature="matrix-export:waves"]')!.click());
    expect(latest.waves).toBeNull();
    expect(host.querySelector('[data-feature="matrix-export:directions"]')).toBeNull();
    expect(host.textContent).toContain("Wave combinations: 1");
  } finally {
    await act(async () => root.unmount());
    host.remove();
  }
});
