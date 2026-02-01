export type PluginMessage =
  | {
      type: "State";
      pluginVersion?: string | null;
      projectFolder?: string | null;
      cachePath?: string | null;
      projectName?: string | null;
      projectSampleRate?: number | null;
      resamplePointsInput?: number | null;
      resamplePointsPitch?: number | null;
      gain?: number | null;
    }
  | {
      type: "ProjectFolderError";
      message: string;
    }
  | {
      type: "ProjectFolderCanceled";
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
    };

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
