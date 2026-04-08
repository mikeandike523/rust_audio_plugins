import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { sendToPluginSafe, useInitializedParam } from "./hooks/useInitializedParam";
import { RESAMPLE_OPTIONS } from "./constants";
import { usePluginMessages } from "./hooks/usePluginMessages";
import { useSampleLoader } from "./hooks/useSampleLoader";
import { useWaveformCanvas } from "./hooks/useWaveformCanvas";
import { clamp } from "./utils/audio";
import type { ResampleModalState, SampleInfo } from "./types/appTypes";
import { Controls } from "./components/Controls";
import { Footer } from "./components/Footer";
import { ProjectCard } from "./components/ProjectCard";
import { ResampleModal } from "./components/ResampleModal";
import { SampleDrop } from "./components/SampleDrop";

export default function App() {
  const [status, setStatus] = useState("Waiting for plugin...");
  const [effectiveCacheDir, setEffectiveCacheDir] = useState<string | null>(null);
  const [cacheDirOverride, setCacheDirOverride] = useState<string | null>(null);
  const [cacheDirError, setCacheDirError] = useState<string | null>(null);
  const [sampleError, setSampleError] = useState<string | null>(null);
  const [sampleInfo, setSampleInfo] = useState<SampleInfo | null>(null);
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

  const pluginVersionParam = useInitializedParam<string>({
    name: "pluginVersion",
    requestPayload: requestStatePayload,
    pollMs: null,
  });

  const projectSampleRateParam = useInitializedParam<number>({
    name: "projectSampleRate",
    requestPayload: requestStatePayload,
    pollMs: null,
  });

  // Stable sendPayload factories — must not be inline arrow fns or they recreate
  // setValue on every render, which destabilises throttled drag handlers.
  const gainSendPayload = useCallback(
    (value: number) => ({ type: "SetGain", value }),
    [],
  );
  const sampleStartSendPayload = useCallback(
    (value: number) => ({ type: "SetSampleStart", value }),
    [],
  );
  const sampleEndSendPayload = useCallback(
    (value: number) => ({ type: "SetSampleEnd", value }),
    [],
  );
  const resampleInputSendPayload = useCallback(
    (value: number) => ({ type: "SetResamplePointsInput", points: value }),
    [],
  );
  const resamplePitchSendPayload = useCallback(
    (value: number) => ({ type: "SetResamplePointsPitch", points: value }),
    [],
  );

  const gainParam = useInitializedParam<number>({
    name: "gain",
    requestPayload: requestStatePayload,
    sendPayload: gainSendPayload,
    pollMs: null,
  });

  const sampleStartParam = useInitializedParam<number>({
    name: "sampleStart",
    initialValue: 0,
    requestPayload: requestStatePayload,
    sendPayload: sampleStartSendPayload,
    pollMs: null,
  });

  const sampleEndParam = useInitializedParam<number>({
    name: "sampleEnd",
    initialValue: 0,
    requestPayload: requestStatePayload,
    sendPayload: sampleEndSendPayload,
    pollMs: null,
  });

  const resamplePointsInputParam = useInitializedParam<number>({
    name: "resamplePointsInput",
    initialValue: RESAMPLE_OPTIONS[2],
    requestPayload: requestStatePayload,
    sendPayload: resampleInputSendPayload,
    pollMs: null,
  });

  const resamplePointsPitchParam = useInitializedParam<number>({
    name: "resamplePointsPitch",
    initialValue: RESAMPLE_OPTIONS[2],
    requestPayload: requestStatePayload,
    sendPayload: resamplePitchSendPayload,
    pollMs: null,
  });

  const getAudioContext = useCallback(() => {
    if (!audioContextRef.current) {
      audioContextRef.current = new AudioContext();
    }
    return audioContextRef.current;
  }, []);

  usePluginMessages({
    pluginVersionParam,
    projectSampleRateParam,
    gainParam,
    sampleStartParam,
    sampleEndParam,
    resamplePointsInputParam,
    resamplePointsPitchParam,
    setStatus,
    setEffectiveCacheDir,
    setCacheDirOverride,
    setCacheDirError,
    setSampleError,
    setSampleInfo,
    setResampleModal,
    setResampleFading,
    audioBufferRef,
    getAudioContext,
  });

  const allParamsReady =
    pluginVersionParam.ready &&
    projectSampleRateParam.ready &&
    gainParam.ready &&
    sampleStartParam.ready &&
    sampleEndParam.ready &&
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

  useWaveformCanvas({
    containerRef: waveformContainerRef,
    canvasRef: waveformCanvasRef,
    audioBufferRef,
    sampleInfo,
  });

  const { handleAudioFile, isDecoding } = useSampleLoader({
    audioBufferRef,
    getAudioContext,
    onSampleInfo: setSampleInfo,
    onSampleError: setSampleError,
    onStatus: setStatus,
  });

  const handlePickCacheDir = () => {
    setCacheDirError(null);
    setStatus("Opening folder picker...");
    sendToPluginSafe({ type: "PickCacheDir" });
  };

  const handleClearCacheDir = () => {
    sendToPluginSafe({ type: "ClearCacheDir" });
  };

  const handleGainChange = (value: number) => {
    const clamped = clamp(value, -24, 24);
    gainParam.setValue(clamped);
  };

  // Destructure stable setValue references so callbacks don't depend on the
  // whole param object (which is a new object reference on every render).
  const { setValue: setSampleStart } = sampleStartParam;
  const { setValue: setSampleEnd } = sampleEndParam;

  const handleSampleStartChange = useCallback(
    (value: number) => {
      setSampleStart(clamp(value, 0, 1));
    },
    [setSampleStart],
  );

  const handleSampleEndChange = useCallback(
    (value: number) => {
      setSampleEnd(clamp(value, 0, 1));
    },
    [setSampleEnd],
  );

  const handleResamplePointsInputChange = (value: number) => {
    if (!Number.isNaN(value)) {
      resamplePointsInputParam.setValue(value);
    }
  };

  const handleResamplePointsPitchChange = (value: number) => {
    if (!Number.isNaN(value)) {
      resamplePointsPitchParam.setValue(value);
    }
  };

  const handleFileSelected = (file: File) => {
    void handleAudioFile(file);
  };

  const handleFileRejected = (message: string, statusText: string) => {
    setSampleError(message);
    setStatus(statusText);
  };

  useEffect(() => {
    if (!sampleInfo) {
      setSampleStart(0);
      setSampleEnd(0);
      return;
    }
    setSampleStart(0);
    setSampleEnd(1);
  }, [sampleInfo, setSampleStart, setSampleEnd]);

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
        <ProjectCard
          effectiveCacheDir={effectiveCacheDir}
          cacheDirOverride={cacheDirOverride}
          projectSampleRate={projectSampleRateParam.value}
          cacheDirError={cacheDirError}
          onPickCacheDir={handlePickCacheDir}
          onClearCacheDir={handleClearCacheDir}
        />

        <SampleDrop
          sampleInfo={sampleInfo}
          sampleError={sampleError}
          isDecoding={isDecoding}
          onFileSelected={handleFileSelected}
          onFileRejected={handleFileRejected}
          sampleStart={sampleStartParam.value}
          sampleEnd={sampleEndParam.value}
          onSampleStartChange={handleSampleStartChange}
          onSampleEndChange={handleSampleEndChange}
          waveformContainerRef={waveformContainerRef}
          waveformCanvasRef={waveformCanvasRef}
        />
      </section>

      <Controls
        gain={gainParam.value}
        onGainChange={handleGainChange}
        resamplePointsInput={resamplePointsInputParam.value}
        resamplePointsPitch={resamplePointsPitchParam.value}
        onResamplePointsInputChange={handleResamplePointsInputChange}
        onResamplePointsPitchChange={handleResamplePointsPitchChange}
      />

      <Footer
        pluginVersion={pluginVersionParam.value}
        guiVersion={guiVersion}
        loadedFrom={loadedFrom}
      />

      <ResampleModal
        resampleModal={resampleModal}
        resampleFading={resampleFading}
      />
    </div>
  );
}
