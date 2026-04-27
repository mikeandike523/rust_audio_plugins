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
      preamp?: number | null;
      gain?: number | null;
      detune?: number | null;
      attack?: number | null;
      decay?: number | null;
      sustain?: number | null;
      release?: number | null;
      bendDepth?: number | null;
      polyphony?: number | null;
      nudgeTo12Edo?: boolean | null;
      referenceFrequencyHz?: number | null;
      detectedPitchHz?: number | null;
      tuningStatus?: {
        active: boolean;
        scl_name?: string | null;
        kbm_name?: string | null;
        error?: string | null;
      } | null;
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
      sample_start?: number | null;
      sample_end?: number | null;
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

export type TuningStatus = {
  active: boolean;
  scl_name?: string | null;
  kbm_name?: string | null;
  error?: string | null;
};

export type ResampleModalState = {
  label: string;
  progress: number;
  message?: string | null;
};
