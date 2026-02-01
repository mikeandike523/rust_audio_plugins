import { useEffect, type MutableRefObject } from "react";

import { drawWaveform } from "../utils/waveform";
import type { SampleInfo } from "../types/appTypes";

type UseWaveformCanvasOptions = {
  containerRef: MutableRefObject<HTMLDivElement | null>;
  canvasRef: MutableRefObject<HTMLCanvasElement | null>;
  audioBufferRef: MutableRefObject<AudioBuffer | null>;
  sampleInfo: SampleInfo | null;
};

export const useWaveformCanvas = ({
  containerRef,
  canvasRef,
  audioBufferRef,
  sampleInfo,
}: UseWaveformCanvasOptions) => {
  useEffect(() => {
    const container = containerRef.current;
    const canvas = canvasRef.current;
    if (!container || !canvas) return;

    const resize = () => {
      const rect = container.getBoundingClientRect();
      const width = Math.max(1, Math.floor(rect.width));
      const height = Math.max(1, Math.floor(rect.height));
      if (canvas.width !== width) {
        canvas.width = width;
      }
      if (canvas.height !== height) {
        canvas.height = height;
      }
      drawWaveform(canvas, audioBufferRef.current);
    };

    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(container);

    return () => observer.disconnect();
  }, [audioBufferRef, canvasRef, containerRef]);

  useEffect(() => {
    drawWaveform(canvasRef.current, audioBufferRef.current);
  }, [audioBufferRef, canvasRef, sampleInfo]);
};
