import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { sendToPluginSafe, useInitializedParam } from "./hooks/useInitializedParam";
import { useLoadingTasks } from "./hooks/useLoadingTasks";
import { usePluginMessages } from "./hooks/usePluginMessages";
import { useSampleLoader } from "./hooks/useSampleLoader";
import { useWaveformCanvas } from "./hooks/useWaveformCanvas";
import { clamp } from "./utils/audio";
import { hzToNoteInfo, formatPitchReadout } from "./utils/pitch";
import type { SampleInfo, TuningStatus } from "./types/appTypes";
import { Controls } from "./components/Controls";
import { Footer } from "./components/Footer";
import { LoadingOverlay } from "./components/LoadingOverlay";
import { SampleDrop } from "./components/SampleDrop";

export default function App() {
  const [status, setStatus] = useState("Waiting for plugin...");
  const [effectiveCacheDir, setEffectiveCacheDir] = useState<string | null>(null);
  const [cacheDirOverride, setCacheDirOverride] = useState<string | null>(null);
  const [cacheDirError, setCacheDirError] = useState<string | null>(null);
  const [sampleError, setSampleError] = useState<string | null>(null);
  const [sampleInfo, setSampleInfo] = useState<SampleInfo | null>(null);
  const [pitchHz, setPitchHz] = useState<number | null>(null);
  const [referenceFrequencyHz, setReferenceFrequencyHz] = useState<number | null>(null);
  const [tuningStatus, setTuningStatus] = useState<TuningStatus | null>(null);
  const [pitchStale, setPitchStale] = useState(false);
  const [loadedFrom] = useState(() => window.location.href);
  const [isOffline, setIsOffline] = useState(() => !navigator.onLine);
  const guiVersion = import.meta.env.VITE_GUI_VERSION ?? "dev";
  const requestStatePayload = useMemo(() => ({ type: "RequestState" }), []);

  const audioContextRef = useRef<AudioContext | null>(null);
  const audioBufferRef = useRef<AudioBuffer | null>(null);
  const waveformContainerRef = useRef<HTMLDivElement | null>(null);
  const waveformCanvasRef = useRef<HTMLCanvasElement | null>(null);

  const { tasks, addTask, updateTask, removeTask } = useLoadingTasks();
  const pitchLoading = tasks.some((t) => t.id === "pitch");

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

  const preampSendPayload = useCallback((value: number) => ({ type: "SetPreamp", value }), []);
  const gainSendPayload = useCallback((value: number) => ({ type: "SetGain", value }), []);
  const detuneSendPayload = useCallback((value: number) => ({ type: "SetDetune", value }), []);
  const sampleStartSendPayload = useCallback((value: number) => ({ type: "SetSampleStart", value }), []);
  const sampleEndSendPayload = useCallback((value: number) => ({ type: "SetSampleEnd", value }), []);
  const resampleInputSendPayload = useCallback(
    (value: number) => ({ type: "SetResampleQualityInput", quality: value }),
    [],
  );
  const resamplePitchSendPayload = useCallback(
    (value: number) => ({ type: "SetResampleQualityPitch", quality: value }),
    [],
  );
  const attackSendPayload = useCallback((value: number) => ({ type: "SetAttack", value }), []);
  const decaySendPayload = useCallback((value: number) => ({ type: "SetDecay", value }), []);
  const sustainSendPayload = useCallback((value: number) => ({ type: "SetSustain", value }), []);
  const releaseSendPayload = useCallback((value: number) => ({ type: "SetRelease", value }), []);
  const bendDepthSendPayload = useCallback((value: number) => ({ type: "SetBendDepth", value }), []);
  const polyphonySendPayload = useCallback((value: number) => ({ type: "SetPolyphony", voices: value }), []);
  const nudgeSendPayload = useCallback((value: boolean) => ({ type: "SetNudgeTo12Edo", enabled: value }), []);

  const preampParam = useInitializedParam<number>({
    name: "preamp",
    initialValue: 0,
    requestPayload: requestStatePayload,
    sendPayload: preampSendPayload,
    pollMs: null,
  });

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

  const attackParam = useInitializedParam<number>({
    name: "attack",
    initialValue: 0.01,
    requestPayload: requestStatePayload,
    sendPayload: attackSendPayload,
    pollMs: null,
  });

  const decayParam = useInitializedParam<number>({
    name: "decay",
    initialValue: 0.1,
    requestPayload: requestStatePayload,
    sendPayload: decaySendPayload,
    pollMs: null,
  });

  const sustainParam = useInitializedParam<number>({
    name: "sustain",
    initialValue: 1,
    requestPayload: requestStatePayload,
    sendPayload: sustainSendPayload,
    pollMs: null,
  });

  const releaseParam = useInitializedParam<number>({
    name: "release",
    initialValue: 0.25,
    requestPayload: requestStatePayload,
    sendPayload: releaseSendPayload,
    pollMs: null,
  });

  const bendDepthParam = useInitializedParam<number>({
    name: "bendDepth",
    initialValue: 200,
    requestPayload: requestStatePayload,
    sendPayload: bendDepthSendPayload,
    pollMs: null,
  });

  const polyphonyParam = useInitializedParam<number>({
    name: "polyphony",
    initialValue: 16,
    requestPayload: requestStatePayload,
    sendPayload: polyphonySendPayload,
    pollMs: null,
  });

  const nudgeTo12EdoParam = useInitializedParam<boolean>({
    name: "nudgeTo12Edo",
    initialValue: false,
    requestPayload: requestStatePayload,
    sendPayload: nudgeSendPayload,
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
    initialValue: 1,
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

  const { setValue: setSampleStart } = sampleStartParam;
  const { setValue: setSampleEnd } = sampleEndParam;

  const getAudioContext = useCallback(() => {
    if (!audioContextRef.current) {
      audioContextRef.current = new AudioContext();
    }
    return audioContextRef.current;
  }, []);

  const handleRecalcPitch = useCallback(() => {
    setPitchStale(false);
    addTask("pitch", "Estimating pitch…");
    sendToPluginSafe({
      type: "RequestPitchEstimate",
      sample_start: sampleStartParam.value ?? 0,
    });
  }, [addTask, sampleStartParam.value]);

  const onSampleSaved = useCallback(() => {
    addTask("pitch", "Estimating pitch…");
    sendToPluginSafe({ type: "RequestPitchEstimate", sample_start: 0 });
    setPitchHz(null);
    setReferenceFrequencyHz(null);
    setPitchStale(false);
  }, [addTask]);

  const onCachedSampleLoaded = useCallback(() => {
    addTask("pitch", "Estimating pitch…");
    sendToPluginSafe({
      type: "RequestPitchEstimate",
      sample_start: sampleStartParam.value ?? 0,
    });
    setPitchStale(false);
  }, [addTask, sampleStartParam.value]);

  usePluginMessages({
    pluginVersionParam,
    projectSampleRateParam,
    preampParam,
    gainParam,
    detuneParam,
    attackParam,
    decayParam,
    sustainParam,
    releaseParam,
    bendDepthParam,
    polyphonyParam,
    nudgeTo12EdoParam,
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
    addTask,
    updateTask,
    removeTask,
    setPitchHz,
    setReferenceFrequencyHz,
    setTuningStatus,
    onSampleSaved,
    onCachedSampleLoaded,
    audioBufferRef,
    getAudioContext,
  });

  useEffect(() => {
    const onOnline = () => setIsOffline(false);
    const onOffline = () => setIsOffline(true);
    window.addEventListener("online", onOnline);
    window.addEventListener("offline", onOffline);
    return () => {
      window.removeEventListener("online", onOnline);
      window.removeEventListener("offline", onOffline);
    };
  }, []);

  useEffect(() => {
    setPitchStale(false);
  }, [pitchHz]);

  const allParamsReady =
    pluginVersionParam.ready &&
    projectSampleRateParam.ready &&
    preampParam.ready &&
    gainParam.ready &&
    detuneParam.ready &&
    attackParam.ready &&
    decayParam.ready &&
    sustainParam.ready &&
    releaseParam.ready &&
    bendDepthParam.ready &&
    polyphonyParam.ready &&
    nudgeTo12EdoParam.ready &&
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
    preampDb: preampParam.value ?? 0,
  });

  const resetClipRange = useCallback(() => {
    setSampleStart(0);
    setSampleEnd(1);
  }, [setSampleEnd, setSampleStart]);

  const { handleAudioFile, isDecoding } = useSampleLoader({
    audioBufferRef,
    getAudioContext,
    onResetClipRange: resetClipRange,
    onSampleInfo: setSampleInfo,
    onSampleError: setSampleError,
    onStatus: setStatus,
  });

  useEffect(() => {
    if (isDecoding) {
      addTask("decode", "Decoding audio…");
    } else {
      removeTask("decode");
    }
  }, [isDecoding, addTask, removeTask]);

  const handlePickCacheDir = () => {
    setCacheDirError(null);
    setStatus("Opening folder picker...");
    sendToPluginSafe({ type: "PickCacheDir" });
  };

  const handleHardRefresh = useCallback(async () => {
    setStatus("Clearing browser caches...");

    try {
      if ("caches" in window) {
        const cacheKeys = await window.caches.keys();
        await Promise.allSettled(cacheKeys.map((key) => window.caches.delete(key)));
      }

      try {
        window.localStorage.clear();
      } catch {
        // Best effort only.
      }

      try {
        window.sessionStorage.clear();
      } catch {
        // Best effort only.
      }

      if ("serviceWorker" in navigator) {
        const registrations = await navigator.serviceWorker.getRegistrations();
        await Promise.allSettled(registrations.map((registration) => registration.unregister()));
      }
    } finally {
      window.location.reload();
    }
  }, []);

  const handleClearCacheDir = () => {
    sendToPluginSafe({ type: "ClearCacheDir" });
  };

  const handleForceResample = () => sendToPluginSafe({ type: "ForceResample" });
  const handlePreampChange = (value: number) => preampParam.setValue(clamp(value, -30, 15));
  const handlePreampReset = () => preampParam.setValue(0);
  const handleGainChange = (value: number) => gainParam.setValue(clamp(value, -24, 24));
  const handleDetuneChange = (value: number) => detuneParam.setValue(clamp(value, -100, 100));
  const handleDetuneReset = () => detuneParam.setValue(0);
  const handleAttackChange = (value: number) => attackParam.setValue(clamp(value, 0, 5));
  const handleDecayChange = (value: number) => decayParam.setValue(clamp(value, 0, 5));
  const handleSustainChange = (value: number) => sustainParam.setValue(clamp(value, 0, 1));
  const handleReleaseChange = (value: number) => releaseParam.setValue(clamp(value, 0, 10));
  const handleBendDepthChange = (value: number) => bendDepthParam.setValue(clamp(value, 100, 400));
  const handlePolyphonyChange = (value: number) => polyphonyParam.setValue(value);

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

  const handleNudgeChange = (checked: boolean) => {
    nudgeTo12EdoParam.setValue(checked);
  };

  const loadTuningFile = useCallback(async (file: File | null, actionType: "SetSclFile" | "SetKbmFile") => {
    if (!file) return;
    const contents = await file.text();
    sendToPluginSafe({ type: actionType, name: file.name, contents });
  }, []);

  const handleSclFileChange = useCallback((file: File | null) => {
    void loadTuningFile(file, "SetSclFile");
  }, [loadTuningFile]);

  const handleKbmFileChange = useCallback((file: File | null) => {
    void loadTuningFile(file, "SetKbmFile");
  }, [loadTuningFile]);

  const isCustomDir = cacheDirOverride !== null;
  const pitchNote = pitchHz !== null ? hzToNoteInfo(pitchHz) : null;

  return (
    <div className="panel">
      <header className="top-bar">
        <div className="top-bar-title">
          <h1>Tunable Sampler</h1>
          <div className="subtitle">Instrument Setup</div>
        </div>
        <div className="top-bar-actions">
          <button
            className="refresh-button"
            type="button"
            onClick={() => {
              void handleHardRefresh();
            }}
            title="Clear browser cache/storage and reload the UI"
            aria-label="Clear browser cache and refresh"
          >
            Clear cache + refresh
          </button>
          <div className="status-chip">{status}</div>
        </div>
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

        <div className="info-block">
          <div className="section-label">Pitch @ Start</div>
          <div className={`pitch-readout${pitchStale ? " is-stale" : ""}`}>
            {pitchLoading
              ? "Estimating…"
              : pitchNote !== null
                ? formatPitchReadout(pitchNote, nudgeTo12EdoParam.value ?? false)
                : "—"}
          </div>
          <label className="nudge-label">
            <input
              type="checkbox"
              checked={nudgeTo12EdoParam.value ?? false}
              onChange={(e) => handleNudgeChange(e.target.checked)}
            />
            Nudge to nearest 12-EDO
          </label>
          <div className="info-meta">
            Reference: {referenceFrequencyHz !== null ? `${referenceFrequencyHz.toFixed(2)} Hz` : "—"}
          </div>
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
        preamp={preampParam.value}
        onPreampChange={handlePreampChange}
        onPreampReset={handlePreampReset}
        gain={gainParam.value}
        onGainChange={handleGainChange}
        detune={detuneParam.value}
        onDetuneChange={handleDetuneChange}
        onDetuneReset={handleDetuneReset}
        attack={attackParam.value}
        onAttackChange={handleAttackChange}
        decay={decayParam.value}
        onDecayChange={handleDecayChange}
        sustain={sustainParam.value}
        onSustainChange={handleSustainChange}
        release={releaseParam.value}
        onReleaseChange={handleReleaseChange}
        bendDepth={bendDepthParam.value}
        onBendDepthChange={handleBendDepthChange}
        polyphony={polyphonyParam.value}
        onPolyphonyChange={handlePolyphonyChange}
        resampleQualityInput={resampleQualityInputParam.value}
        resampleQualityPitch={resampleQualityPitchParam.value}
        onResampleQualityInputChange={(v) => resampleQualityInputParam.setValue(v)}
        onResampleQualityPitchChange={(v) => resampleQualityPitchParam.setValue(v)}
        onForceResample={handleForceResample}
        tuningStatus={tuningStatus}
        onSclFileChange={handleSclFileChange}
        onKbmFileChange={handleKbmFileChange}
        onClearSclFile={() => sendToPluginSafe({ type: "ClearSclFile" })}
        onClearKbmFile={() => sendToPluginSafe({ type: "ClearKbmFile" })}
      />

      <Footer
        pluginVersion={pluginVersionParam.value}
        guiVersion={guiVersion}
        loadedFrom={loadedFrom}
      />

      <LoadingOverlay tasks={tasks} />
      {isOffline && (
        <div className="offline-banner">
          Offline — loaded from PWA cache · {loadedFrom}
        </div>
      )}
    </div>
  );
}
