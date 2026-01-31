import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type ChangeEvent,
  type DragEvent,
} from "react";
import {
  sendToPluginSafe,
  useInitializedParam,
} from "./hooks/useInitializedParam";

type PluginMessage =
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

type SampleInfo = {
  name: string;
  sampleRate: number;
  channels: number;
  frames: number;
  duration: number;
};

type ResampleModalState = {
  label: string;
  progress: number;
  status: "working" | "done" | "error";
  message?: string;
};

const clamp = (value: number, min: number, max: number) =>
  Math.min(max, Math.max(min, value));

const RESAMPLE_OPTIONS = [128, 256, 512, 1024, 2048] as const;

const arrayBufferToBase64 = (buffer: ArrayBuffer) => {
  const bytes = new Uint8Array(buffer);
  const chunkSize = 0x8000;
  let binary = "";
  for (let i = 0; i < bytes.length; i += chunkSize) {
    const chunk = bytes.subarray(i, i + chunkSize);
    binary += String.fromCharCode(...chunk);
  }
  return window.btoa(binary);
};

const base64ToFloat32Array = (dataBase64: string) => {
  const binary = window.atob(dataBase64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return new Float32Array(bytes.buffer);
};

const drawWaveform = (
  canvas: HTMLCanvasElement | null,
  audioBuffer: AudioBuffer | null,
) => {
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;

  const width = canvas.width;
  const height = canvas.height;

  ctx.clearRect(0, 0, width, height);

  if (!audioBuffer) {
    return;
  }

  const frames = audioBuffer.length;
  const channels = audioBuffer.numberOfChannels;
  const mid = height / 2;
  const padding = 12;
  const usableHeight = Math.max(0, mid - padding);

  ctx.strokeStyle = "#e07a3f";
  ctx.lineWidth = 1;
  ctx.beginPath();

  const samplesPerPixel = Math.max(1, Math.floor(frames / width));
  for (let x = 0; x < width; x += 1) {
    const start = x * samplesPerPixel;
    const end = Math.min(frames, start + samplesPerPixel);
    let peak = 0;
    for (let ch = 0; ch < channels; ch += 1) {
      const data = audioBuffer.getChannelData(ch);
      for (let i = start; i < end; i += 1) {
        const abs = Math.abs(data[i]);
        if (abs > peak) peak = abs;
      }
    }
    const amp = peak * usableHeight;
    const xPos = x + 0.5;
    ctx.moveTo(xPos, mid - amp);
    ctx.lineTo(xPos, mid + amp);
  }

  ctx.stroke();

  ctx.strokeStyle = "rgba(26, 26, 24, 0.18)";
  ctx.beginPath();
  ctx.moveTo(0, mid + 0.5);
  ctx.lineTo(width, mid + 0.5);
  ctx.stroke();
};

export default function App() {
  const [status, setStatus] = useState("Waiting for plugin...");
  const [cacheFolder, setCacheFolder] = useState<string | null>(null);
  const [folderError, setFolderError] = useState<string | null>(null);
  const [sampleError, setSampleError] = useState<string | null>(null);
  const [sampleInfo, setSampleInfo] = useState<SampleInfo | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [isDecoding, setIsDecoding] = useState(false);
  const [resampleModal, setResampleModal] =
    useState<ResampleModalState | null>(null);
  const [resampleFading, setResampleFading] = useState(false);
  const [loadedFrom] = useState(() => window.location.href);
  const guiVersion = import.meta.env.VITE_GUI_VERSION ?? "dev";
  const requestStatePayload = useMemo(() => ({ type: "RequestState" }), []);

  const audioContextRef = useRef<AudioContext | null>(null);
  const audioBufferRef = useRef<AudioBuffer | null>(null);
  const waveformContainerRef = useRef<HTMLDivElement | null>(null);
  const waveformCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const resampleTimeoutRef = useRef<number | null>(null);

  const pluginVersionParam = useInitializedParam<string>({
    name: "pluginVersion",
    requestPayload: requestStatePayload,
    pollMs: null,
  });

  const projectFolderParam = useInitializedParam<string>({
    name: "projectFolder",
    requestPayload: requestStatePayload,
    sendPayload: (value) => ({ type: "SetProjectFolder", path: value }),
    pollMs: null,
  });

  const projectNameParam = useInitializedParam<string>({
    name: "projectName",
    requestPayload: requestStatePayload,
    pollMs: null,
  });

  const projectSampleRateParam = useInitializedParam<number>({
    name: "projectSampleRate",
    requestPayload: requestStatePayload,
    pollMs: null,
  });

  const gainParam = useInitializedParam<number>({
    name: "gain",
    requestPayload: requestStatePayload,
    sendPayload: (value) => ({ type: "SetGain", value }),
    pollMs: null,
  });

  const resamplePointsInputParam = useInitializedParam<number>({
    name: "resamplePointsInput",
    initialValue: RESAMPLE_OPTIONS[2],
    requestPayload: requestStatePayload,
    sendPayload: (value) => ({ type: "SetResamplePointsInput", points: value }),
    pollMs: null,
  });

  const resamplePointsPitchParam = useInitializedParam<number>({
    name: "resamplePointsPitch",
    initialValue: RESAMPLE_OPTIONS[2],
    requestPayload: requestStatePayload,
    sendPayload: (value) => ({ type: "SetResamplePointsPitch", points: value }),
    pollMs: null,
  });

  const needsProjectFolder = projectFolderParam.value === null;

  useEffect(() => {
    (window as { onPluginMessage?: Function }).onPluginMessage = (
      message: PluginMessage,
    ) => {
      if (message.type === "State") {
        let nextStatus = "Connected";
        if (typeof message.pluginVersion === "string") {
          pluginVersionParam.setFromPlugin(message.pluginVersion);
        }
        if (message.projectFolder === null) {
          projectFolderParam.setFromPlugin(null);
        } else if (typeof message.projectFolder === "string") {
          projectFolderParam.setFromPlugin(message.projectFolder);
          setFolderError(null);
          nextStatus = "Project folder set";
        }
        if (message.cachePath === null) {
          setCacheFolder(null);
        } else if (typeof message.cachePath === "string") {
          setCacheFolder(message.cachePath);
        }
        if (message.projectName === null) {
          projectNameParam.setFromPlugin(null);
        } else if (typeof message.projectName === "string") {
          projectNameParam.setFromPlugin(message.projectName);
        }
        if (message.projectSampleRate === null) {
          projectSampleRateParam.setFromPlugin(null);
        } else if (typeof message.projectSampleRate === "number") {
          projectSampleRateParam.setFromPlugin(
            Math.round(message.projectSampleRate),
          );
        }
        if (message.resamplePointsInput === null) {
          resamplePointsInputParam.setFromPlugin(null);
        } else if (typeof message.resamplePointsInput === "number") {
          resamplePointsInputParam.setFromPlugin(message.resamplePointsInput);
        }
        if (message.resamplePointsPitch === null) {
          resamplePointsPitchParam.setFromPlugin(null);
        } else if (typeof message.resamplePointsPitch === "number") {
          resamplePointsPitchParam.setFromPlugin(message.resamplePointsPitch);
        }
        if (message.gain === null) {
          gainParam.setFromPlugin(null);
        } else if (typeof message.gain === "number") {
          gainParam.setFromPlugin(clamp(message.gain, -24, 24));
        }
        setStatus(nextStatus);
      }

      if (message.type === "ProjectFolderError") {
        setFolderError(message.message);
        setStatus("Project folder error");
      }

      if (message.type === "ProjectFolderCanceled") {
        setStatus("Folder picker canceled");
      }

      if (message.type === "SampleSaved") {
        setSampleError(null);
        setStatus(`Sample cached${message.name ? `: ${message.name}` : ""}`);
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
            `Sample loaded from cache${
              message.name ? `: ${message.name}` : ""
            }`,
          );
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
  }, []);

  const allParamsReady =
    pluginVersionParam.ready &&
    projectFolderParam.ready &&
    projectNameParam.ready &&
    projectSampleRateParam.ready &&
    gainParam.ready &&
    resamplePointsInputParam.ready &&
    resamplePointsPitchParam.ready;

  useEffect(() => {
    if (allParamsReady) {
      return;
    }

    sendToPluginSafe(requestStatePayload);
    const intervalId = window.setInterval(() => {
      sendToPluginSafe(requestStatePayload);
    }, 200);

    return () => {
      window.clearInterval(intervalId);
    };
  }, [allParamsReady, requestStatePayload]);

  useEffect(() => {
    const container = waveformContainerRef.current;
    const canvas = waveformCanvasRef.current;
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
  }, []);

  useEffect(() => {
    drawWaveform(waveformCanvasRef.current, audioBufferRef.current);
  }, [sampleInfo]);

  const getAudioContext = () => {
    if (!audioContextRef.current) {
      audioContextRef.current = new AudioContext();
    }
    return audioContextRef.current;
  };

  const handleProjectFolderPicker = () => {
    setFolderError(null);
    setStatus("Opening folder picker...");
    sendToPluginSafe({ type: "PickProjectFolder" });
  };

  const handleGainChange = (value: number) => {
    const clamped = clamp(value, -24, 24);
    gainParam.setValue(clamped);
  };

  const handleResamplePointsInputChange = (
    event: ChangeEvent<HTMLSelectElement>,
  ) => {
    const points = Number(event.target.value);
    if (!Number.isNaN(points)) {
      resamplePointsInputParam.setValue(points);
    }
  };

  const handleResamplePointsPitchChange = (
    event: ChangeEvent<HTMLSelectElement>,
  ) => {
    const points = Number(event.target.value);
    if (!Number.isNaN(points)) {
      resamplePointsPitchParam.setValue(points);
    }
  };

  const handleAudioFile = async (file: File) => {
    if (!cacheFolder) {
      setSampleError("Select a project folder before loading audio.");
      setStatus("Project folder required");
      return;
    }

    setSampleError(null);
    setIsDecoding(true);
    setStatus(`Decoding ${file.name}...`);

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

      setSampleInfo({
        name: file.name,
        sampleRate,
        channels,
        frames,
        duration: frames / sampleRate,
      });

      setStatus(`Sample loaded: ${file.name}`);
    } catch (err) {
      const message =
        err instanceof Error ? err.message : "Failed to decode audio file.";
      setSampleError(message);
      setStatus("Sample decode failed");
    } finally {
      setIsDecoding(false);
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
      setSampleError("Please drop a supported audio file.");
      setStatus("Unsupported file type");
      return;
    }

    void handleAudioFile(file);
  };

  const handleFilePicker = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (file) {
      void handleAudioFile(file);
    }
    event.target.value = "";
  };

  return (
    <div className="panel">
      <header className="top-bar">
        <div>
          <h1>Tunable Sampler</h1>
          <div className="subtitle">Instrument Setup</div>
        </div>
        <div className="status-chip">{status}</div>
      </header>

      <section className="workspace">
        <div className="project-card">
          <div className="section-label">Project Folder</div>
          <div className="project-path">
            {projectFolderParam.value ?? "No folder selected"}
          </div>
          {projectNameParam.value ? (
            <div className="project-meta">Project: {projectNameParam.value}</div>
          ) : null}
          {cacheFolder ? (
            <div className="project-meta">Cache: {cacheFolder}</div>
          ) : null}
          <div className="project-meta">
            Host sample rate:{" "}
            {projectSampleRateParam.value === null
              ? "--"
              : `${projectSampleRateParam.value} Hz`}
          </div>
          {folderError ? (
            <div className="project-error">{folderError}</div>
          ) : null}
          <div className="project-actions">
            <button
              className="pick-button"
              type="button"
              onClick={handleProjectFolderPicker}
            >
              {projectFolderParam.value ? "Change Folder" : "Pick Folder"}
            </button>
          </div>
        </div>

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
              {!sampleInfo ? (
                <div className="drop-placeholder">Drop audio here</div>
              ) : null}
              {isDecoding ? (
                <div className="drop-loading">Decoding...</div>
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
              {sampleError ? (
                <div className="sample-error">{sampleError}</div>
              ) : null}
            </div>
          </div>
        </div>
      </section>

      <section className="controls">
        <div className="control">
          <label htmlFor="gain">Gain</label>
          <input
            id="gain"
            type="range"
            min="-24"
            max="24"
            step="0.1"
            value={gainParam.value ?? 0}
            onChange={(event) => handleGainChange(Number(event.target.value))}
            disabled={gainParam.value === null}
          />
          <div className="value">
            {gainParam.value === null
              ? "--"
              : `${gainParam.value.toFixed(1)} dB`}
          </div>
        </div>

        <div className="control">
          <label htmlFor="resample-input">
            Resample Points (Project Match)
          </label>
          <select
            id="resample-input"
            value={resamplePointsInputParam.value ?? RESAMPLE_OPTIONS[2]}
            onChange={handleResamplePointsInputChange}
          >
            {RESAMPLE_OPTIONS.map((option) => (
              <option key={option} value={option}>
                {option} points
              </option>
            ))}
          </select>
          <div className="value">
            {resamplePointsInputParam.value === null
              ? "--"
              : `${resamplePointsInputParam.value} points`}
          </div>
        </div>

        <div className="control">
          <label htmlFor="resample-pitch">
            Resample Points (Pitch Adjust)
          </label>
          <select
            id="resample-pitch"
            value={resamplePointsPitchParam.value ?? RESAMPLE_OPTIONS[2]}
            onChange={handleResamplePointsPitchChange}
          >
            {RESAMPLE_OPTIONS.map((option) => (
              <option key={option} value={option}>
                {option} points
              </option>
            ))}
          </select>
          <div className="value">
            {resamplePointsPitchParam.value === null
              ? "--"
              : `${resamplePointsPitchParam.value} points`}
          </div>
        </div>
      </section>

      <footer className="footer">
        <div className="version-meta">
          <div>plugin-version: {pluginVersionParam.value ?? "unknown"}</div>
          <div>gui-version: {guiVersion}</div>
        </div>
        <div className="source">
          <div className="source-label">Loaded From</div>
          <div className="source-value" title={loadedFrom}>
            {loadedFrom}
          </div>
        </div>
      </footer>

      {resampleModal ? (
        <div
          className={`progress-backdrop${
            resampleFading ? " is-fading" : ""
          }`}
        >
          <div className="progress-modal" role="status" aria-live="polite">
            <div className="progress-title">{resampleModal.label}</div>
            <div className="progress-bar">
              <div
                className="progress-fill"
                style={{
                  width: `${Math.round(resampleModal.progress * 100)}%`,
                }}
              />
            </div>
            <div className="progress-copy">
              {resampleModal.message ??
                `${Math.round(resampleModal.progress * 100)}%`}
            </div>
          </div>
        </div>
      ) : null}

      {needsProjectFolder ? (
        <div className="modal-backdrop">
          <div
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="project-folder-required-title"
          >
            <div className="modal-title" id="project-folder-required-title">
              Select a project folder to continue
            </div>
            <div className="modal-copy">
              This sampler needs your DAW project folder before the rest of the
              controls unlock.
            </div>
            <div className="path-input modal-input">
              <button
                className="pick-button"
                type="button"
                onClick={handleProjectFolderPicker}
              >
                Pick Folder
              </button>
            </div>
            {folderError ? (
              <div className="modal-error">{folderError}</div>
            ) : null}
          </div>
        </div>
      ) : null}
    </div>
  );
}
