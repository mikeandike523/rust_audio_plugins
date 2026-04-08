export type PluginMessage =
  | {
      type: "State";
      pluginVersion?: string | null;
      effectiveCacheDir?: string | null;
      cacheDirOverride?: string | null;
      projectSampleRate?: number | null;
      resampleQualityInput?: number | null;
      resampleQualityPitch?: number | null;
      sampleStart?: number | null;
      sampleEnd?: number | null;
      gain?: number | null;
      detune?: number | null;
    }
  | {
      type: "CacheDirError";
      message: string;
    }
  | {
      type: "CacheDirCanceled";
    }
  | {
      type: "SampleSaved";
      name?: string | null;
    }
  | {
      type: "SampleSaveError";
      message: string;
    }
  | {
      type: "CachedSample";
      name?: string | null;
      sample_rate: number;
      channels: number;
      frames: number;
      data_base64: string;
    }
  | {
      type: "CachedSampleError";
      message: string;
    }
  | {
      type: "ResampleStarted";
      label: string;
      progress?: number | null;
    }
  | {
      type: "ResampleProgress";
      progress: number;
    }
  | {
      type: "ResampleComplete";
      message?: string | null;
    }
  | {
      type: "ResampleError";
      message: string;
    }
  | { type: "PitchEstimating" }
  | { type: "PitchDetected"; hz: number }
  | { type: "PitchNoResult" }
  | { type: "PitchEstimateError"; message: string };

export type SampleInfo = {
  name: string;
  sampleRate: number;
  channels: number;
  frames: number;
  duration: number;
};

export type ResampleModalState = {
  label: string;
  progress: number;
  status: "working" | "done" | "error";
  message?: string;
};
