import lodash from "lodash";
import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type ChangeEvent,
  type DragEvent,
  type PointerEvent,
  type MutableRefObject,
} from "react";

import type { SampleInfo } from "../types/appTypes";
import { clamp } from "../utils/audio";

type SampleDropProps = {
  sampleInfo: SampleInfo | null;
  sampleError: string | null;
  isDecoding: boolean;
  onFileSelected: (file: File) => void;
  onFileRejected: (message: string, status: string) => void;
  sampleStart: number | null;
  sampleEnd: number | null;
  onSampleStartChange: (value: number) => void;
  onSampleEndChange: (value: number) => void;
  waveformContainerRef: MutableRefObject<HTMLDivElement | null>;
  waveformCanvasRef: MutableRefObject<HTMLCanvasElement | null>;
};

export const SampleDrop = ({
  sampleInfo,
  sampleError,
  isDecoding,
  onFileSelected,
  onFileRejected,
  sampleStart,
  sampleEnd,
  onSampleStartChange,
  onSampleEndChange,
  waveformContainerRef,
  waveformCanvasRef,
}: SampleDropProps) => {
  const [isDragging, setIsDragging] = useState(false);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const dragHandleRef = useRef<"start" | "end" | null>(null);
  const dragPointerRef = useRef<number | null>(null);
  const dragBoundsRef = useRef<{ left: number; width: number } | null>(null);
  const startValueRef = useRef(0);
  const endValueRef = useRef(0);

  const throttledSetStart = useMemo(
    () =>
      lodash.throttle(
        (value: number) => {
          onSampleStartChange(value);
        },
        80,
        { leading: true, trailing: true },
      ),
    [onSampleStartChange],
  );

  const throttledSetEnd = useMemo(
    () =>
      lodash.throttle(
        (value: number) => {
          onSampleEndChange(value);
        },
        80,
        { leading: true, trailing: true },
      ),
    [onSampleEndChange],
  );

  useEffect(() => {
    return () => {
      throttledSetStart.cancel();
      throttledSetEnd.cancel();
    };
  }, [throttledSetEnd, throttledSetStart]);

  useEffect(() => {
    startValueRef.current = clamp(sampleStart ?? 0, 0, 1);
  }, [sampleStart]);

  useEffect(() => {
    endValueRef.current = clamp(sampleEnd ?? 0, 0, 1);
  }, [sampleEnd]);

  const setDragBounds = () => {
    const bounds = waveformContainerRef.current?.getBoundingClientRect();
    if (!bounds || bounds.width <= 0) {
      dragBoundsRef.current = null;
      return null;
    }
    const nextBounds = { left: bounds.left, width: bounds.width };
    dragBoundsRef.current = nextBounds;
    return nextBounds;
  };

  const updateFromClientX = (
    clientX: number,
    handle: "start" | "end",
    flush: boolean,
  ) => {
    const bounds = dragBoundsRef.current ?? setDragBounds();
    if (!bounds) return;
    const ratio = clamp((clientX - bounds.left) / bounds.width, 0, 1);

    if (handle === "start") {
      const next = Math.min(ratio, endValueRef.current);
      startValueRef.current = next;
      if (flush) {
        throttledSetStart.cancel();
        onSampleStartChange(next);
      } else {
        throttledSetStart(next);
      }
      return;
    }

    const next = Math.max(ratio, startValueRef.current);
    endValueRef.current = next;
    if (flush) {
      throttledSetEnd.cancel();
      onSampleEndChange(next);
    } else {
      throttledSetEnd(next);
    }
  };

  const handleDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    event.stopPropagation();
    setIsDragging(false);

    const file = event.dataTransfer.files?.[0];
    if (!file) {
      return;
    }

    if (!file.type.startsWith("audio/")) {
      onFileRejected("Please drop a supported audio file.", "Unsupported file type");
      return;
    }

    onFileSelected(file);
  };

  const handleFilePicker = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (file) {
      onFileSelected(file);
    }
    event.target.value = "";
  };

  const handlePointerDown = (
    event: PointerEvent<HTMLDivElement>,
    handle: "start" | "end",
  ) => {
    if (!sampleInfo) return;
    event.preventDefault();
    event.stopPropagation();
    setDragBounds();
    dragHandleRef.current = handle;
    dragPointerRef.current = event.pointerId;
    event.currentTarget.setPointerCapture(event.pointerId);
    updateFromClientX(event.clientX, handle, true);
  };

  const handlePointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (dragPointerRef.current !== event.pointerId) {
      return;
    }
    const handle = dragHandleRef.current;
    if (!handle) return;
    updateFromClientX(event.clientX, handle, false);
  };

  const handlePointerUp = (event: PointerEvent<HTMLDivElement>) => {
    if (dragPointerRef.current !== event.pointerId) {
      return;
    }
    const handle = dragHandleRef.current;
    if (handle) {
      updateFromClientX(event.clientX, handle, true);
    }
    dragHandleRef.current = null;
    dragPointerRef.current = null;
    setDragBounds();
  };

  const handlePointerCancel = (event: PointerEvent<HTMLDivElement>) => {
    if (dragPointerRef.current !== event.pointerId) {
      return;
    }
    dragHandleRef.current = null;
    dragPointerRef.current = null;
    throttledSetStart.cancel();
    throttledSetEnd.cancel();
    setDragBounds();
  };

  const startValue = clamp(sampleStart ?? 0, 0, 1);
  const endValue = clamp(sampleEnd ?? 0, 0, 1);
  const startPercent = startValue * 100;
  const endPercent = endValue * 100;
  const shadeLeft = Math.min(startValue, endValue) * 100;
  const shadeRight = Math.max(startValue, endValue) * 100;
  const leftShadeWidth = Math.max(0, Math.min(shadeLeft, 100));
  const rightShadeLeft = Math.max(0, Math.min(shadeRight, 100));
  const rightShadeWidth = Math.max(0, 100 - rightShadeLeft);

  return (
    <div
      className={`sample-drop${isDragging ? " is-dragging" : ""}${
        sampleInfo ? " has-sample" : ""
      }${isDecoding ? " is-decoding" : ""}`}
      onDragEnter={(event) => {
        event.preventDefault();
        setIsDragging(true);
      }}
      onDragOver={(event) => {
        event.preventDefault();
        event.dataTransfer.dropEffect = "copy";
      }}
      onDragLeave={() => {
        setIsDragging(false);
      }}
      onDrop={handleDrop}
      onClick={() => fileInputRef.current?.click()}
      role="button"
      tabIndex={0}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          fileInputRef.current?.click();
        }
      }}
    >
      <input
        ref={fileInputRef}
        className="hidden-file-input"
        type="file"
        accept="audio/*"
        onChange={handleFilePicker}
      />
      <div className="sample-drop-inner">
        <div className="waveform-shell">
          <div className="waveform" ref={waveformContainerRef}>
            <canvas ref={waveformCanvasRef} />
            {sampleInfo ? (
              <div className="clip-overlay">
                <div
                  className="clip-shade clip-shade-left"
                  style={{ width: `${leftShadeWidth}%` }}
                />
                <div
                  className="clip-shade clip-shade-right"
                  style={{
                    left: `${rightShadeLeft}%`,
                    width: `${rightShadeWidth}%`,
                  }}
                />
                <div
                  className="clip-line clip-line-start"
                  style={{ left: `${startPercent}%` }}
                />
                <div
                  className="clip-line clip-line-end"
                  style={{ left: `${endPercent}%` }}
                />
              </div>
            ) : null}
            {!sampleInfo ? <div className="drop-placeholder">Drop audio here</div> : null}
            {isDecoding ? <div className="drop-loading">Decoding...</div> : null}
          </div>
          {sampleInfo ? (
            <div className="clip-handles">
              <div
                className="clip-handle clip-handle-start"
                style={{ left: `${startPercent}%` }}
                onPointerDown={(event) => handlePointerDown(event, "start")}
                onPointerMove={handlePointerMove}
                onPointerUp={handlePointerUp}
                onPointerCancel={handlePointerCancel}
                onLostPointerCapture={handlePointerCancel}
                onClick={(event) => event.stopPropagation()}
                role="slider"
                aria-label="Sample start"
              >
                s
              </div>
              <div
                className="clip-handle clip-handle-end"
                style={{ left: `${endPercent}%` }}
                onPointerDown={(event) => handlePointerDown(event, "end")}
                onPointerMove={handlePointerMove}
                onPointerUp={handlePointerUp}
                onPointerCancel={handlePointerCancel}
                onLostPointerCapture={handlePointerCancel}
                onClick={(event) => event.stopPropagation()}
                role="slider"
                aria-label="Sample end"
              >
                e
              </div>
            </div>
          ) : null}
        </div>
        <div className="sample-meta">
          <div className="sample-name">
            {sampleInfo?.name ?? "No sample loaded"}
          </div>
          <div className="sample-details">
            {sampleInfo
              ? `${sampleInfo.channels} ch / ${Math.round(
                  sampleInfo.sampleRate,
                )} Hz / ${sampleInfo.duration.toFixed(2)} s`
              : "Drag & drop audio (wav, mp3, ogg, etc.)"}
          </div>
          {sampleError ? <div className="sample-error">{sampleError}</div> : null}
        </div>
      </div>
    </div>
  );
};
