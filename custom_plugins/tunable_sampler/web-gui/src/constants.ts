export const RESAMPLE_QUALITY_OPTIONS = [
  { value: 0, label: "Normal", sinc_len: 32, oversampling: 64 },
  { value: 1, label: "High", sinc_len: 64, oversampling: 128 },
  { value: 2, label: "Ultra High", sinc_len: 128, oversampling: 256 },
] as const;

export type ResampleQualityValue = (typeof RESAMPLE_QUALITY_OPTIONS)[number]["value"];
