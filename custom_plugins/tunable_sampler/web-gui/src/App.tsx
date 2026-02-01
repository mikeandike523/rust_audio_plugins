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
import { ProjectFolderModal } from "./components/ProjectFolderModal";
import { ResampleModal } from "./components/ResampleModal";
import { SampleDrop } from "./components/SampleDrop";

export default function App() {
  const [status, setStatus] = useState("Waiting for plugin...");
  const [cacheFolder, setCacheFolder] = useState<string | null>(null);
  const [folderError, setFolderError] = useState<string | null>(null);
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

  const getAudioContext = useCallback(() => {
    if (!audioContextRef.current) {
      audioContextRef.current = new AudioContext();
    }
    return audioContextRef.current;
  }, []);

  usePluginMessages({
    pluginVersionParam,
    projectFolderParam,
    projectNameParam,
    projectSampleRateParam,
    gainParam,
    resamplePointsInputParam,
    resamplePointsPitchParam,
    setStatus,
    setCacheFolder,
    setFolderError,
    setSampleError,
    setSampleInfo,
    setResampleModal,
    setResampleFading,
    audioBufferRef,
    getAudioContext,
  });

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

  useWaveformCanvas({
    containerRef: waveformContainerRef,
    canvasRef: waveformCanvasRef,
    audioBufferRef,
    sampleInfo,
  });

  const { handleAudioFile, isDecoding } = useSampleLoader({
    cacheFolder,
    audioBufferRef,
    getAudioContext,
    onSampleInfo: setSampleInfo,
    onSampleError: setSampleError,
    onStatus: setStatus,
  });

  const handleProjectFolderPicker = () => {
    setFolderError(null);
    setStatus("Opening folder picker...");
    sendToPluginSafe({ type: "PickProjectFolder" });
  };

  const handleGainChange = (value: number) => {
    const clamped = clamp(value, -24, 24);
    gainParam.setValue(clamped);
  };

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
          projectFolder={projectFolderParam.value}
          projectName={projectNameParam.value}
          cacheFolder={cacheFolder}
          projectSampleRate={projectSampleRateParam.value}
          folderError={folderError}
          onPickFolder={handleProjectFolderPicker}
        />

        <SampleDrop
          sampleInfo={sampleInfo}
          sampleError={sampleError}
          isDecoding={isDecoding}
          onFileSelected={handleFileSelected}
          onFileRejected={handleFileRejected}
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

      <ProjectFolderModal
        show={needsProjectFolder}
        folderError={folderError}
        onPickFolder={handleProjectFolderPicker}
      />
    </div>
  );
}
