import {
  useEffect,
  useRef,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from "react";

import { sendToPluginSafe } from "./useInitializedParam";
import { base64ToFloat32Array, clamp } from "../utils/audio";
import type { ResampleModalState, SampleInfo } from "../types/appTypes";
import type { PluginMessage } from "../types/appTypes";

type InitializedParam<T> = {
  ready: boolean;
  setFromPlugin: (value: T | null) => void;
  setValue: (value: T) => void;
  value: T | null;
};

type UsePluginMessagesOptions = {
  pluginVersionParam: InitializedParam<string>;
  projectSampleRateParam: InitializedParam<number>;
  gainParam: InitializedParam<number>;
  detuneParam: InitializedParam<number>;
  sampleStartParam: InitializedParam<number>;
  sampleEndParam: InitializedParam<number>;
  resampleQualityInputParam: InitializedParam<number>;
  resampleQualityPitchParam: InitializedParam<number>;
  setStatus: Dispatch<SetStateAction<string>>;
  setEffectiveCacheDir: Dispatch<SetStateAction<string | null>>;
  setCacheDirOverride: Dispatch<SetStateAction<string | null>>;
  setCacheDirError: Dispatch<SetStateAction<string | null>>;
  setSampleError: Dispatch<SetStateAction<string | null>>;
  setSampleInfo: Dispatch<SetStateAction<SampleInfo | null>>;
  setResampleModal: Dispatch<SetStateAction<ResampleModalState | null>>;
  setResampleFading: Dispatch<SetStateAction<boolean>>;
  setPitchHz: Dispatch<SetStateAction<number | null>>;
  setPitchLoading: Dispatch<SetStateAction<boolean>>;
  onSampleSaved: () => void;
  onCachedSampleLoaded: () => void;
  audioBufferRef: MutableRefObject<AudioBuffer | null>;
  getAudioContext: () => AudioContext;
};

export const usePluginMessages = (options: UsePluginMessagesOptions) => {
  const resampleTimeoutRef = useRef<number | null>(null);

  // "Latest ref" pattern: always holds the most recent options so the effect
  // can register the handler exactly once (on mount) without stale closures.
  const optionsRef = useRef(options);
  optionsRef.current = options;

  useEffect(() => {
    (window as { onPluginMessage?: (message: PluginMessage) => void })
      .onPluginMessage = (message) => {
      const {
        pluginVersionParam,
        projectSampleRateParam,
        gainParam,
        detuneParam,
        sampleStartParam,
        sampleEndParam,
        resampleQualityInputParam,
        resampleQualityPitchParam,
        setStatus,
        setEffectiveCacheDir,
        setCacheDirOverride,
        setCacheDirError,
        setSampleError,
        setSampleInfo,
        setResampleModal,
        setResampleFading,
        setPitchHz,
        setPitchLoading,
        onSampleSaved,
        onCachedSampleLoaded,
        audioBufferRef,
        getAudioContext,
      } = optionsRef.current;

      if (message.type === "State") {
        let nextStatus = "Connected";
        if (typeof message.pluginVersion === "string") {
          pluginVersionParam.setFromPlugin(message.pluginVersion);
        }
        if (message.effectiveCacheDir === null) {
          setEffectiveCacheDir(null);
        } else if (typeof message.effectiveCacheDir === "string") {
          setEffectiveCacheDir(message.effectiveCacheDir);
          setCacheDirError(null);
        }
        if ("cacheDirOverride" in message) {
          setCacheDirOverride(message.cacheDirOverride ?? null);
        }
        if (message.projectSampleRate === null) {
          projectSampleRateParam.setFromPlugin(null);
        } else if (typeof message.projectSampleRate === "number") {
          projectSampleRateParam.setFromPlugin(
            Math.round(message.projectSampleRate),
          );
        }
        if (message.resampleQualityInput === null) {
          resampleQualityInputParam.setFromPlugin(null);
        } else if (typeof message.resampleQualityInput === "number") {
          resampleQualityInputParam.setFromPlugin(message.resampleQualityInput);
        }
        if (message.resampleQualityPitch === null) {
          resampleQualityPitchParam.setFromPlugin(null);
        } else if (typeof message.resampleQualityPitch === "number") {
          resampleQualityPitchParam.setFromPlugin(message.resampleQualityPitch);
        }
        if (message.sampleStart === null) {
          sampleStartParam.setFromPlugin(null);
        } else if (typeof message.sampleStart === "number") {
          sampleStartParam.setFromPlugin(clamp(message.sampleStart, 0, 1));
        }
        if (message.sampleEnd === null) {
          sampleEndParam.setFromPlugin(null);
        } else if (typeof message.sampleEnd === "number") {
          sampleEndParam.setFromPlugin(clamp(message.sampleEnd, 0, 1));
        }
        if (message.gain === null) {
          gainParam.setFromPlugin(null);
        } else if (typeof message.gain === "number") {
          gainParam.setFromPlugin(clamp(message.gain, -24, 24));
        }
        if (message.detune === null) {
          detuneParam.setFromPlugin(null);
        } else if (typeof message.detune === "number") {
          detuneParam.setFromPlugin(clamp(message.detune, -100, 100));
        }
        setStatus(nextStatus);
      }

      if (message.type === "CacheDirError") {
        setCacheDirError(message.message);
        setStatus("Cache dir error");
      }

      if (message.type === "CacheDirCanceled") {
        setStatus("Folder picker canceled");
      }

      if (message.type === "SampleSaved") {
        setSampleError(null);
        setStatus(`Sample cached${message.name ? `: ${message.name}` : ""}`);
        onSampleSaved();
      }

      if (message.type === "SampleSaveError") {
        setSampleError(message.message);
        setStatus("Sample save error");
      }

      if (message.type === "CachedSample") {
        if (audioBufferRef.current) return;

        try {
          const interleaved = base64ToFloat32Array(message.data_base64);
          const expectedLength = message.frames * message.channels;
          if (interleaved.length !== expectedLength) {
            throw new Error(
              `Sample cache size mismatch (expected ${expectedLength} frames, got ${interleaved.length}).`,
            );
          }

          const ctx = getAudioContext();
          const audioBuffer = ctx.createBuffer(
            message.channels,
            message.frames,
            message.sample_rate,
          );
          for (let ch = 0; ch < message.channels; ch += 1) {
            const channelData = audioBuffer.getChannelData(ch);
            for (let i = 0; i < message.frames; i += 1) {
              channelData[i] = interleaved[i * message.channels + ch];
            }
          }
          audioBufferRef.current = audioBuffer;

          setSampleInfo({
            name: message.name ?? "Cached sample",
            sampleRate: message.sample_rate,
            channels: message.channels,
            frames: message.frames,
            duration: message.frames / message.sample_rate,
          });
          setSampleError(null);
          setStatus(
            `Sample loaded from cache${message.name ? `: ${message.name}` : ""}`,
          );
          onCachedSampleLoaded();
        } catch (err) {
          const errorMessage =
            err instanceof Error
              ? err.message
              : "Failed to load cached sample.";
          setSampleError(errorMessage);
          setStatus("Cached sample load error");
        }
      }

      if (message.type === "CachedSampleError") {
        setSampleError(message.message);
        setStatus("Cached sample error");
      }

      if (message.type === "ResampleStarted") {
        if (resampleTimeoutRef.current) {
          window.clearTimeout(resampleTimeoutRef.current);
          resampleTimeoutRef.current = null;
        }
        setResampleFading(false);
        setResampleModal({
          label: message.label,
          progress: message.progress ?? 0,
          status: "working",
        });
      }

      if (message.type === "ResampleProgress") {
        setResampleModal((prev) => {
          if (!prev) {
            return {
              label: "Resampling...",
              progress: message.progress,
              status: "working",
            };
          }
          return {
            ...prev,
            progress: message.progress,
            status: "working",
          };
        });
      }

      if (message.type === "ResampleComplete") {
        setResampleModal((prev) => ({
          label: prev?.label ?? "Resample complete",
          progress: 1,
          status: "done",
          message: message.message ?? undefined,
        }));
        setResampleFading(true);
        resampleTimeoutRef.current = window.setTimeout(() => {
          setResampleModal(null);
          setResampleFading(false);
          resampleTimeoutRef.current = null;
        }, 800);
      }

      if (message.type === "ResampleError") {
        setResampleModal({
          label: "Resample failed",
          progress: 1,
          status: "error",
          message: message.message,
        });
        setResampleFading(false);
        resampleTimeoutRef.current = window.setTimeout(() => {
          setResampleModal(null);
          resampleTimeoutRef.current = null;
        }, 1600);
      }

      if (message.type === "PitchEstimating") {
        setPitchLoading(true);
      }

      if (message.type === "PitchDetected") {
        setPitchHz(message.hz);
        setPitchLoading(false);
      }

      if (message.type === "PitchNoResult") {
        setPitchHz(null);
        setPitchLoading(false);
      }

      if (message.type === "PitchEstimateError") {
        setPitchLoading(false);
      }
    };

    sendToPluginSafe({ type: "Init" });

    return () => {
      if (window.onPluginMessage) {
        window.onPluginMessage = undefined;
      }
      if (resampleTimeoutRef.current) {
        window.clearTimeout(resampleTimeoutRef.current);
        resampleTimeoutRef.current = null;
      }
    };
  // Empty deps: register handler exactly once on mount. All runtime values are
  // read via optionsRef.current so the handler is never stale.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
};
