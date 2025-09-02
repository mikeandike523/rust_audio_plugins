export interface OscillatorParams {
  v: number;
  a: number;
  d: number;
  s: number;
  r: number;
}

export interface NXODefinition {
  [frequencyMultiplier: string|number]: OscillatorParams;
}

export function isNXODefinition(obj: unknown): obj is NXODefinition {
  if (obj === null || typeof obj !== 'object' || Array.isArray(obj)) {
    return false;
  }

  const entries = Object.entries(obj as Record<string, unknown>);

  // Object must be non-empty
  if (entries.length === 0) {
    return false;
  }

  // Validate frequency multiplier keys (they're strings at runtime but must parse to valid numbers)
  for (const [key] of entries) {
    const freq = Number(key);
    if (!Number.isFinite(freq)) {
      return false;
    }
  }

  // Validate each oscillator has exactly v, a, d, s, r fields
  const requiredFields = ['v', 'a', 'd', 's', 'r'];
  
  for (const [, oscillatorParams] of entries) {
    if (typeof oscillatorParams !== 'object' || oscillatorParams === null || Array.isArray(oscillatorParams)) {
      return false;
    }

    const paramObj = oscillatorParams as Record<string, unknown>;
    const paramKeys = Object.keys(paramObj);

    // Check if it has exactly the required fields (no more, no less)
    if (paramKeys.length !== requiredFields.length ||
        !requiredFields.every(field => paramKeys.includes(field))) {
      return false;
    }

    // Check if all required values are finite numbers
    for (const field of requiredFields) {
      const value = paramObj[field];
      if (typeof value !== 'number' || !Number.isFinite(value)) {
        return false;
      }
    }
  }

  return true;
}