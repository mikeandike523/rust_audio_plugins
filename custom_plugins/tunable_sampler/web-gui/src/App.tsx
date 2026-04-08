import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { sendToPluginSafe, useInitializedParam } from "./hooks/useInitializedParam";
import { usePluginMessages } from "./hooks/usePluginMessages";
import { useSampleLoader } from "./hooks/useSampleLoader";
import { useWaveformCanvas } from "./hooks/useWaveformCanvas";
import { clamp } from "./utils/audio";
import { hzToNoteInfo, formatPitchReadout } from "./utils/pitch";
import type { ResampleModalState, SampleInfo } from "./types/appTypes";
import { Controls } from "./components/Controls";
import { Footer } from "./components/Footer";
import { ResampleModal } from "./components/ResampleModal";
import { SampleDrop } from "./components/SampleDrop";

export default function App() {
  const [status, setStatus] = useState("Waiting for plugin...");
  const [effectiveCacheDir, setEffectiveCacheDir] = useState<string | null>(null);
  const [cacheDirOverride, setCacheDirOverride] = useState<string | null>(null);
  const [cacheDirError, setCacheDirError] = useState<string | null>(null);
  const [sampleError, setSampleError] = useState<string | null>(null);
  const [sampleInfo, setSampleInfo] = useState<SampleInfo | null>(null);
  const [resampleModal, setResampleModal] = useState<ResampleModalState | null>(null);
  const [resampleFading, setResampleFading] = useState(false);
  const [pitchHz, setPitchHz] = useState<number | null>(null);
  const [pitchLoading, setPitchLoading] = useState(false);
  // true when nudge checkbox changed (or start pos changed) since the last estimate
  const [pitchStale, setPitchStale] = useState(false);
  const [nudgeTo12edo, setNudgeTo12edo] = useState(false);
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
  const detuneSendPayload = useCallback(
    (value: number) => ({ type: "SetDetune", value }),
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
    (value: number) => ({ type: "SetResampleQualityInput", quality: value }),
    [],
  );
  const resamplePitchSendPayload = useCallback(
    (value: number) => ({ type: "SetResampleQualityPitch", quality: value }),
    [],
  );

  const gainParam = useInitializedParam<number>({
    name: "gain",
    requestPayload: requestStatePayload,
    sendPayload: gainSendPayload,
    pollMs: null,
  });

  const detuneParam = useInitializedParam<number>({
    name: "detune",
    initialValue: 0,
    requestPayload: requestStatePayload,
    sendPayload: detuneSendPayload,
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

  const resampleQualityInputParam = useInitializedParam<number>({
    name: "resampleQualityInput",
    initialValue: 2,
    requestPayload: requestStatePayload,
    sendPayload: resampleInputSendPayload,
    pollMs: null,
  });

  const resampleQualityPitchParam = useInitializedParam<number>({
    name: "resampleQualityPitch",
    initialValue: 0,
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

  const handleRecalcPitch = useCallback(() => {
    setPitchStale(false);
    sendToPluginSafe({
      type: "RequestPitchEstimate",
      sample_start: sampleStartParam.value ?? 0,
    });
    setPitchLoading(true);
  }, [sampleStartParam.value]);

  const onSampleSaved = useCallback(() => {
    // Auto-estimate pitch from position 0 on each new sample save.
    // Start handle resets to 0 via the sampleInfo effect below.
    sendToPluginSafe({ type: "RequestPitchEstimate", sample_start: 0 });
    setPitchLoading(true);
    setPitchHz(null);
    setPitchStale(false);
  }, []);

  const onCachedSampleLoaded = useCallback(() => {
    sendToPluginSafe({
      type: "RequestPitchEstimate",
      sample_start: sampleStartParam.value ?? 0,
    });
    setPitchLoading(true);
    setPitchStale(false);
  }, [sampleStartParam.value]);

  usePluginMessages({
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
  });

  // Clear stale whenever a pitch result arrives (detected or no-result).
  useEffect(() => {
    setPitchStale(false);
  }, [pitchHz]);

  const allParamsReady =
    pluginVersionParam.ready &&
    projectSampleRateParam.ready &&
    gainParam.ready &&
    detuneParam.ready &&
    sampleStartParam.ready &&
    sampleEndParam.ready &&
    resampleQualityInputParam.ready &&
    resampleQualityPitchParam.ready;

  useEffect(() => {
    if (allParamsReady) return;
    sendToPluginSafe(requestStatePayload);
    const id = window.setInterval(() => sendToPluginSafe(requestStatePayload), 200);
    return () => window.clearInterval(id);
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

  const handleForceResample = () => sendToPluginSafe({ type: "ForceResample" });

  const handleGainChange = (value: number) => gainParam.setValue(clamp(value, -24, 24));

  const handleDetuneChange = (value: number) =>
    detuneParam.setValue(clamp(value, -100, 100));
  const handleDetuneReset = () => detuneParam.setValue(0);

  const { setValue: setSampleStart } = sampleStartParam;
  const { setValue: setSampleEnd } = sampleEndParam;

  const handleSampleStartChange = useCallback(
    (value: number) => setSampleStart(clamp(value, 0, 1)),
    [setSampleStart],
  );
  const handleSampleEndChange = useCallback(
    (value: number) => setSampleEnd(clamp(value, 0, 1)),
    [setSampleEnd],
  );

  const handleFileSelected = (file: File) => void handleAudioFile(file);
  const handleFileRejected = (message: string, statusText: string) => {
    setSampleError(message);
    setStatus(statusText);
  };

  // Reset handles to full range when a new sample is loaded.
  useEffect(() => {
    if (!sampleInfo) {
      setSampleStart(0);
      setSampleEnd(0);
      return;
    }
    setSampleStart(0);
    setSampleEnd(1);
  }, [sampleInfo, setSampleStart, setSampleEnd]);

  const handleNudgeChange = (checked: boolean) => {
    setNudgeTo12edo(checked);
    // Require recalculate after toggling nudge mode (same UX as changing start pos).
    if (pitchHz !== null) {
      setPitchStale(true);
    }
  };

  const isCustomDir = cacheDirOverride !== null;
  const pitchNote = pitchHz !== null ? hzToNoteInfo(pitchHz) : null;

  return (
    <div className="panel">
      <header className="top-bar">
        <div className="top-bar-title">
          <h1>Tunable Sampler</h1>
          <div className="subtitle">Instrument Setup</div>
        </div>
        <div className="status-chip">{status}</div>
      </header>

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

      <div className="info-strip">
        {/* Cache dir */}
        <div className="info-block">
          <div className="section-label">Cache{isCustomDir ? " · custom" : ""}</div>
          <div className="info-path" title={effectiveCacheDir ?? undefined}>
            {effectiveCacheDir ?? "Resolving…"}
          </div>
          <div className="info-meta">
            Host rate: {projectSampleRateParam.value !== null ? `${projectSampleRateParam.value} Hz` : "—"}
          </div>
          {cacheDirError && <div className="info-error">{cacheDirError}</div>}
          <div className="info-actions">
            <button className="mini-button" type="button" onClick={handlePickCacheDir}>
              {isCustomDir ? "Change" : "Set Custom"}
            </button>
            {isCustomDir && (
              <button className="mini-button" type="button" onClick={handleClearCacheDir}>
                Default
              </button>
            )}
          </div>
        </div>

        {/* Sample meta */}
        <div className="info-block">
          <div className="section-label">Sample</div>
          <div className="info-name">{sampleInfo?.name ?? "No sample loaded"}</div>
          {sampleInfo && (
            <div className="info-meta">
              {sampleInfo.channels}ch · {Math.round(sampleInfo.sampleRate)} Hz · {sampleInfo.duration.toFixed(2)}s
            </div>
          )}
          {sampleError && <div className="info-error">{sampleError}</div>}
        </div>

        {/* Pitch */}
        <div className="info-block">
          <div className="section-label">Pitch @ Start</div>
          <div className={`pitch-readout${pitchStale ? " is-stale" : ""}`}>
            {pitchLoading
              ? "Estimating…"
              : pitchNote !== null
                ? formatPitchReadout(pitchNote, nudgeTo12edo)
                : "—"}
          </div>
          <label className="nudge-label">
            <input
              type="checkbox"
              checked={nudgeTo12edo}
              onChange={(e) => handleNudgeChange(e.target.checked)}
            />
            Nudge to nearest 12-EDO
          </label>
          {sampleInfo && (
            <button
              className={`mini-button${pitchStale ? " needs-update" : ""}`}
              type="button"
              onClick={handleRecalcPitch}
              disabled={pitchLoading}
            >
              {pitchStale ? "Recalculate ↻" : "Recalculate Pitch@Start"}
            </button>
          )}
        </div>
      </div>

      <Controls
        gain={gainParam.value}
        onGainChange={handleGainChange}
        detune={detuneParam.value}
        onDetuneChange={handleDetuneChange}
        onDetuneReset={handleDetuneReset}
        resampleQualityInput={resampleQualityInputParam.value}
        resampleQualityPitch={resampleQualityPitchParam.value}
        onResampleQualityInputChange={(v) => resampleQualityInputParam.setValue(v)}
        onResampleQualityPitchChange={(v) => resampleQualityPitchParam.setValue(v)}
        onForceResample={handleForceResample}
      />

      <Footer
        pluginVersion={pluginVersionParam.value}
        guiVersion={guiVersion}
        loadedFrom={loadedFrom}
      />

      <ResampleModal resampleModal={resampleModal} resampleFading={resampleFading} />
    </div>
  );
}
