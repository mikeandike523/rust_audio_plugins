import { useEffect, useRef, type MutableRefObject } from "react";

import { drawWaveform } from "../utils/waveform";
import type { SampleInfo } from "../types/appTypes";

type UseWaveformCanvasOptions = {
  containerRef: MutableRefObject<HTMLDivElement | null>;
  canvasRef: MutableRefObject<HTMLCanvasElement | null>;
  audioBufferRef: MutableRefObject<AudioBuffer | null>;
  sampleInfo: SampleInfo | null;
  preampDb: number;
};

export const useWaveformCanvas = ({
  containerRef,
  canvasRef,
  audioBufferRef,
  sampleInfo,
  preampDb,
}: UseWaveformCanvasOptions) => {
  const preampDbRef = useRef(preampDb);
  preampDbRef.current = preampDb;

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
      drawWaveform(canvas, audioBufferRef.current, preampDbRef.current);
    };

    let raf1 = 0;
    let raf2 = 0;
    let timeout1 = 0;
    let timeout2 = 0;
    let timeout3 = 0;

    const scheduleRedrawBurst = () => {
      resize();
      raf1 = window.requestAnimationFrame(() => {
        resize();
        raf2 = window.requestAnimationFrame(resize);
      });
      timeout1 = window.setTimeout(resize, 0);
      timeout2 = window.setTimeout(resize, 60);
      timeout3 = window.setTimeout(resize, 180);
    };

    resize();
    scheduleRedrawBurst();
    const observer = new ResizeObserver(resize);
    observer.observe(container);
    const handleVisibilityChange = () => {
      if (!document.hidden) {
        scheduleRedrawBurst();
      }
    };
    document.addEventListener("visibilitychange", handleVisibilityChange);

    return () => {
      observer.disconnect();
      document.removeEventListener("visibilitychange", handleVisibilityChange);
      window.cancelAnimationFrame(raf1);
      window.cancelAnimationFrame(raf2);
      window.clearTimeout(timeout1);
      window.clearTimeout(timeout2);
      window.clearTimeout(timeout3);
    };
  }, [audioBufferRef, canvasRef, containerRef]);

  useEffect(() => {
    drawWaveform(canvasRef.current, audioBufferRef.current, preampDb);
    const timeoutId = window.setTimeout(() => {
      drawWaveform(canvasRef.current, audioBufferRef.current, preampDb);
    }, 0);
    return () => window.clearTimeout(timeoutId);
  }, [audioBufferRef, canvasRef, sampleInfo, preampDb]);
};
