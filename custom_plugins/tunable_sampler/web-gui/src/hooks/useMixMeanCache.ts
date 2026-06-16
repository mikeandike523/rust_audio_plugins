import { useEffect, useRef, useState, type MutableRefObject } from "react";

import type { SampleInfo } from "../types/appTypes";

type UseMixMeanCacheOptions = {
  audioBufferRef: MutableRefObject<AudioBuffer | null>;
  sampleInfo: SampleInfo | null;
  channelMode: number;
  addTask: (id: string, message: string) => void;
  removeTask: (id: string) => void;
};

const MIXMEAN_TASK_ID = "mixmean";

/**
 * Lazily builds and caches the (L+R)*0.5 mono mix used by the MixMean waveform.
 *
 * - Computed only the first time MixMean is selected for a given sample (or when a
 *   new sample loads while MixMean is active) — never up-front, so projects that
 *   never touch MixMean pay nothing.
 * - Preamp-independent, so dragging the preamp sliders never invalidates it.
 * - Invalidated whenever the loaded sample changes.
 * - Drives the shared loading-circle system while the array is being built, since
 *   long samples can take a noticeable moment.
 */
export const useMixMeanCache = ({
  audioBufferRef,
  sampleInfo,
  channelMode,
  addTask,
  removeTask,
}: UseMixMeanCacheOptions): Float32Array | null => {
  const [mixMeanData, setMixMeanData] = useState<Float32Array | null>(null);
  // The sample the current cache was built for; used to invalidate on change.
  const builtForRef = useRef<SampleInfo | null>(null);

  // Invalidate when the sample changes (runs before the compute effect below).
  useEffect(() => {
    if (builtForRef.current !== sampleInfo) {
      builtForRef.current = null;
      setMixMeanData(null);
    }
  }, [sampleInfo]);

  useEffect(() => {
    if (channelMode !== 1) return;
    if (!sampleInfo) return;
    if (builtForRef.current === sampleInfo && mixMeanData) return;
    const buffer = audioBufferRef.current;
    if (!buffer) return;

    let cancelled = false;
    addTask(MIXMEAN_TASK_ID, "Preparing mono mix…");
    // Defer one tick so the loading indicator paints before the (blocking) build.
    const timeoutId = window.setTimeout(() => {
      if (cancelled) return;
      const frames = buffer.length;
      const numCh = buffer.numberOfChannels;
      const l = buffer.getChannelData(0);
      const r = numCh > 1 ? buffer.getChannelData(1) : l;
      const out = new Float32Array(frames);
      for (let i = 0; i < frames; i += 1) {
        out[i] = (l[i] + r[i]) * 0.5;
      }
      builtForRef.current = sampleInfo;
      setMixMeanData(out);
      removeTask(MIXMEAN_TASK_ID);
    }, 0);

    return () => {
      cancelled = true;
      window.clearTimeout(timeoutId);
      removeTask(MIXMEAN_TASK_ID);
    };
  }, [channelMode, sampleInfo, mixMeanData, audioBufferRef, addTask, removeTask]);

  return mixMeanData;
};
