export const RESAMPLE_OPTIONS = [128, 256, 512, 1024, 2048] as const;
export type ResampleOption = (typeof RESAMPLE_OPTIONS)[number];
