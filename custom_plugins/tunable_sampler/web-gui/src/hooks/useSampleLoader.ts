import { useCallback, useState, type MutableRefObject } from "react";

import { sendToPluginSafe } from "./useInitializedParam";
import { arrayBufferToBase64 } from "../utils/audio";
import type { SampleInfo } from "../types/appTypes";

type UseSampleLoaderOptions = {
  cacheFolder: string | null;
  audioBufferRef: MutableRefObject<AudioBuffer | null>;
  getAudioContext: () => AudioContext;
  onSampleInfo: (info: SampleInfo) => void;
  onSampleError: (message: string | null) => void;
  onStatus: (status: string) => void;
};

export const useSampleLoader = ({
  cacheFolder,
  audioBufferRef,
  getAudioContext,
  onSampleInfo,
  onSampleError,
  onStatus,
}: UseSampleLoaderOptions) => {
  const [isDecoding, setIsDecoding] = useState(false);

  const handleAudioFile = useCallback(
    async (file: File) => {
      if (!cacheFolder) {
        onSampleError("Select a project folder before loading audio.");
        onStatus("Project folder required");
        return;
      }

      onSampleError(null);
      setIsDecoding(true);
      onStatus(`Decoding ${file.name}...`);

      try {
        const arrayBuffer = await file.arrayBuffer();
        const ctx = getAudioContext();
        const audioBuffer = await ctx.decodeAudioData(arrayBuffer.slice(0));
        audioBufferRef.current = audioBuffer;

        const channels = audioBuffer.numberOfChannels;
        const frames = audioBuffer.length;
        const sampleRate = audioBuffer.sampleRate;

        const interleaved = new Float32Array(frames * channels);
        for (let ch = 0; ch < channels; ch += 1) {
          const data = audioBuffer.getChannelData(ch);
          for (let i = 0; i < frames; i += 1) {
            interleaved[i * channels + ch] = data[i];
          }
        }

        const dataBase64 = arrayBufferToBase64(interleaved.buffer);
        sendToPluginSafe({
          type: "SaveSample",
          name: file.name,
          sample_rate: Math.round(sampleRate),
          channels,
          frames,
          data_base64: dataBase64,
        });

        onSampleInfo({
          name: file.name,
          sampleRate,
          channels,
          frames,
          duration: frames / sampleRate,
        });

        onStatus(`Sample loaded: ${file.name}`);
      } catch (err) {
        const message =
          err instanceof Error ? err.message : "Failed to decode audio file.";
        onSampleError(message);
        onStatus("Sample decode failed");
      } finally {
        setIsDecoding(false);
      }
    },
    [
      audioBufferRef,
      cacheFolder,
      getAudioContext,
      onSampleError,
      onSampleInfo,
      onStatus,
    ],
  );

  return { handleAudioFile, isDecoding };
};
