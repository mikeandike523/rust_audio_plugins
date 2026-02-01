import {
  useRef,
  useState,
  type ChangeEvent,
  type DragEvent,
  type MutableRefObject,
} from "react";

import type { SampleInfo } from "../types/appTypes";

type SampleDropProps = {
  sampleInfo: SampleInfo | null;
  sampleError: string | null;
  isDecoding: boolean;
  onFileSelected: (file: File) => void;
  onFileRejected: (message: string, status: string) => void;
  waveformContainerRef: MutableRefObject<HTMLDivElement | null>;
  waveformCanvasRef: MutableRefObject<HTMLCanvasElement | null>;
};

export const SampleDrop = ({
  sampleInfo,
  sampleError,
  isDecoding,
  onFileSelected,
  onFileRejected,
  waveformContainerRef,
  waveformCanvasRef,
}: SampleDropProps) => {
  const [isDragging, setIsDragging] = useState(false);
  const fileInputRef = useRef<HTMLInputElement | null>(null);

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
        <div className="waveform" ref={waveformContainerRef}>
          <canvas ref={waveformCanvasRef} />
          {!sampleInfo ? <div className="drop-placeholder">Drop audio here</div> : null}
          {isDecoding ? <div className="drop-loading">Decoding...</div> : null}
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
