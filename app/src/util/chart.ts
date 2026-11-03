/** Y-axis range and tick values for a weight chart (port of the Flet progress view). */
export function axisTicks(lo: number, hi: number): { yMin: number; yMax: number; ticks: number[]; step: number } {
  const span = Math.max(hi - lo, 1);
  const step = span <= 4 ? 1 : span <= 12 ? 2.5 : span <= 30 ? 5 : 10;
  const yMin = Math.max(0, Math.floor((lo - step * 0.5) / step) * step);
  const yMax = (Math.floor((hi + step * 0.5) / step) + 1) * step;
  const ticks: number[] = [];
  for (let v = yMin; v <= yMax + 1e-9; v += step) ticks.push(Math.round(v * 100) / 100);
  return { yMin, yMax, ticks, step };
}
