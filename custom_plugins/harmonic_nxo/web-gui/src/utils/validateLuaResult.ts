export interface HarmonicParams {
  [key: string]: unknown;
}

export interface HarmonicResult {
  [frequency: number]: HarmonicParams;
}

export function isHarmonicResult(obj: unknown): obj is HarmonicResult {
  if (obj === null || typeof obj !== 'object') return false;
  for (const [k, v] of Object.entries(obj as Record<string, unknown>)) {
    const freq = Number(k);
    if (!Number.isFinite(freq)) return false;
    if (typeof v !== 'object' || v === null || Array.isArray(v)) return false;
  }
  return true;
}
