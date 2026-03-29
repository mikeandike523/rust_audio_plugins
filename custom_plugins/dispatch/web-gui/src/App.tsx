import { useEffect, useRef, useState } from "react";

type PluginMessage =
  | {
      type: "State";
      cacheDirOverride?: string | null;
      effectiveCacheDir?: string;
      pluginVersion?: string;
      baseVoices?: number;
      allowInfiniteVoices?: boolean;
      retrigger?: boolean;
      masterGain?: number;
      velSensDb?: number;
      padVolumes?: number[];
      padMonos?: boolean[];
      padNormalizes?: boolean[];
    }
  | { type: "SampleLoaded"; padIndex: number; name: string }
  | { type: "SampleError"; padIndex: number; message: string }
  | { type: "PadName"; padIndex: number; name: string }
  | { type: "PadCleared"; padIndex: number };

type PadState = { name: string; loading: boolean } | null;

const sendToPluginSafe = (payload: unknown) => {
  if (typeof window.sendToPlugin === "function") {
    window.sendToPlugin(payload);
  } else {
    console.info("sendToPlugin not available", payload);
  }
};

const PAD_COUNT = 16;
const PAD_MIDI_BASE = 36;

// Decode a dropped audio file to interleaved f32 base64
async function decodeAudioFile(file: File): Promise<{
  sampleRate: number;
  channels: number;
  frames: number;
  dataBase64: string;
}> {
  const arrayBuffer = await file.arrayBuffer();
  const audioCtx = new AudioContext();
  let audioBuffer: AudioBuffer;
  try {
    audioBuffer = await audioCtx.decodeAudioData(arrayBuffer);
  } finally {
    audioCtx.close();
  }

  const { numberOfChannels, length, sampleRate } = audioBuffer;
  const channelArrays = Array.from({ length: numberOfChannels }, (_, i) =>
    audioBuffer.getChannelData(i)
  );

  const interleaved = new Float32Array(length * numberOfChannels);
  for (let frame = 0; frame < length; frame++) {
    for (let ch = 0; ch < numberOfChannels; ch++) {
      interleaved[frame * numberOfChannels + ch] = channelArrays[ch][frame];
    }
  }

  const bytes = new Uint8Array(interleaved.buffer);
  let binary = "";
  const chunkSize = 0x8000;
  for (let i = 0; i < bytes.length; i += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunkSize));
  }

  return { sampleRate, channels: numberOfChannels, frames: length, dataBase64: btoa(binary) };
}

// ---------------------------------------------------------------------------
// Cache Dir Bar
// ---------------------------------------------------------------------------

function CacheDirBar({
  cacheDirOverride,
  effectiveCacheDir,
  onSetCustomDir,
  onRemoveCustomDir,
}: {
  cacheDirOverride: string | null;
  effectiveCacheDir: string;
  onSetCustomDir: (path: string) => void;
  onRemoveCustomDir: () => void;
}) {
  const [editing, setEditing] = useState(false);
  const [input, setInput] = useState("");

  const startEdit = () => {
    setInput(cacheDirOverride ?? "");
    setEditing(true);
  };

  const confirmEdit = () => {
    const t = input.trim();
    if (t) {
      onSetCustomDir(t);
    }
    setEditing(false);
  };

  const handleRemove = () => {
    const ok = window.confirm(
      "Remove the custom cache directory override?\n\n" +
      "The plugin will revert to the default location:\n" +
      "  " + effectiveCacheDir + "\n\n" +
      "Samples already in the old custom directory will NOT be moved. " +
      "Any pads pointing to that directory will be unloaded."
    );
    if (ok) onRemoveCustomDir();
  };

  return (
    <div className="cache-bar">
      <span className="cache-bar-label">cache</span>

      {editing ? (
        <>
          <input
            className="cache-bar-input"
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") confirmEdit();
              if (e.key === "Escape") setEditing(false);
            }}
            autoFocus
            spellCheck={false}
          />
          <button className="cache-bar-btn cache-bar-btn--confirm" onClick={confirmEdit} disabled={!input.trim()}>
            ok
          </button>
          <button className="cache-bar-btn" onClick={() => setEditing(false)}>
            cancel
          </button>
        </>
      ) : (
        <>
          <span
            className={`cache-bar-path ${cacheDirOverride ? "cache-bar-path--custom" : "cache-bar-path--default"}`}
            title={effectiveCacheDir}
          >
            {cacheDirOverride ? effectiveCacheDir : `${effectiveCacheDir} (default)`}
          </span>
          <button className="cache-bar-btn" onClick={startEdit} title="Set a custom cache directory">
            set custom
          </button>
          {cacheDirOverride && (
            <button className="cache-bar-btn cache-bar-btn--danger" onClick={handleRemove} title="Revert to default cache directory">
              remove custom
            </button>
          )}
        </>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Pad Grid
// ---------------------------------------------------------------------------

function PadGrid({
  activePads,
  padStates,
  onSampleDrop,
  onDeletePad,
}: {
  activePads: Set<number>;
  padStates: PadState[];
  onSampleDrop: (padIndex: number, file: File) => void;
  onDeletePad: (padIndex: number) => void;
}) {
  const [dragOverPad, setDragOverPad] = useState<number | null>(null);

  return (
    <div className="pad-grid">
      {Array.from({ length: PAD_COUNT }, (_, i) => {
        const midiNote = PAD_MIDI_BASE + i;
        const noteNames = ["C","C#","D","D#","E","F","F#","G","G#","A","A#","B"];
        const noteName = `${noteNames[midiNote % 12]}${Math.floor(midiNote / 12) - 1}`;
        const isActive = activePads.has(i);
        const isDragOver = dragOverPad === i;
        const pad = padStates[i];

        return (
          <div
            key={i}
            className={[
              "pad",
              isActive ? "pad--active" : "",
              isDragOver ? "pad--drag-over" : "",
              pad?.loading ? "pad--loading" : "",
            ]
              .filter(Boolean)
              .join(" ")}
            title={`MIDI ${midiNote}`}
            onDragOver={(e) => {
              e.preventDefault();
              e.dataTransfer.dropEffect = "copy";
              if (dragOverPad !== i) setDragOverPad(i);
            }}
            onDragLeave={() => setDragOverPad(null)}
            onDrop={(e) => {
              e.preventDefault();
              setDragOverPad(null);
              const file = e.dataTransfer.files[0];
              if (file) onSampleDrop(i, file);
            }}
          >
            <div className="pad-label">
              {pad ? (
                pad.name
              ) : (
                <span className="pad-label--empty">{i + 1}</span>
              )}
              {pad && !pad.loading && (
                <button
                  className="pad-delete-btn"
                  onClick={(e) => { e.stopPropagation(); onDeletePad(i); }}
                  title="Remove sample"
                >
                  ×
                </button>
              )}
            </div>
            <div className="pad-note">{noteName}</div>
            <div className={`pad-sample ${pad ? "" : "pad-sample--empty"}`}>
              {pad?.loading ? "resampling…" : pad ? pad.name : "drop sample"}
            </div>
          </div>
        );
      })}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Mixer
// ---------------------------------------------------------------------------

function Mixer({
  padStates,
  masterGain,
  velSensDb,
  padVolumes,
  padMonos,
  padNormalizes,
  onMasterGainChange,
  onVelSensChange,
  onVolumeChange,
  onMonoChange,
  onNormalizeChange,
}: {
  padStates: PadState[];
  masterGain: number;
  velSensDb: number;
  padVolumes: number[];
  padMonos: boolean[];
  padNormalizes: boolean[];
  onMasterGainChange: (gain: number) => void;
  onVelSensChange: (sensDb: number) => void;
  onVolumeChange: (padIndex: number, volume: number) => void;
  onMonoChange: (padIndex: number, mono: boolean) => void;
  onNormalizeChange: (padIndex: number, normalize: boolean) => void;
}) {
  const fmtGain = (db: number) =>
    db === 0 ? "0 dB" : `${db > 0 ? "+" : ""}${db.toFixed(1)} dB`;

  return (
    <div className="mixer-section">
      {Array.from({ length: PAD_COUNT }, (_, i) => (
        <div key={i} className="mixer-channel">
          <span className="mixer-label">
            {padStates[i]?.name ?? String(i + 1)}
          </span>
          <div className="mixer-fader-wrap">
            <input
              type="range"
              className="mixer-fader"
              min={0}
              max={2}
              step={0.01}
              value={padVolumes[i]}
              onChange={(e) => onVolumeChange(i, Number(e.target.value))}
            />
          </div>
          <div className="mixer-btns">
            <button
              className={`mixer-btn${padNormalizes[i] ? " mixer-btn--on" : ""}`}
              onClick={() => onNormalizeChange(i, !padNormalizes[i])}
              title="Normalize (pre-scale to peak = 1.0)"
            >
              N
            </button>
            <button
              className={`mixer-btn${padMonos[i] ? " mixer-btn--on" : ""}`}
              onClick={() => onMonoChange(i, !padMonos[i])}
              title="Force mono"
            >
              M
            </button>
          </div>
          <span className="mixer-val">
            {padVolumes[i] === 1.0
              ? "0dB"
              : padVolumes[i] === 0
              ? "−∞"
              : `${((padVolumes[i] - 1) * 100).toFixed(0)}%`}
          </span>
        </div>
      ))}

      <div className="mixer-master">
        <span className="mixer-label">OUT</span>
        <div className="mixer-fader-wrap">
          <input
            type="range"
            className="mixer-fader"
            min={-15}
            max={9}
            step={0.1}
            value={masterGain}
            onChange={(e) => onMasterGainChange(Number(e.target.value))}
          />
        </div>
        <span className="mixer-val">{fmtGain(masterGain)}</span>
      </div>

      <div className="mixer-master">
        <span className="mixer-label">VEL</span>
        <div className="mixer-fader-wrap">
          <input
            type="range"
            className="mixer-fader"
            min={-60}
            max={0}
            step={0.5}
            value={velSensDb}
            onChange={(e) => onVelSensChange(Number(e.target.value))}
            title="Velocity sensitivity: at 0 dB velocity has no effect; at −60 dB full velocity range applies"
          />
        </div>
        <span className="mixer-val">
          {velSensDb >= 0 ? "off" : `${velSensDb.toFixed(0)}dB`}
        </span>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main App
// ---------------------------------------------------------------------------

export default function App() {
  const [cacheDirOverride, setCacheDirOverride] = useState<string | null>(null);
  const [effectiveCacheDir, setEffectiveCacheDir] = useState<string>("");
  const [pluginVersion, setPluginVersion] = useState<string | null>(null);
  const [activePads] = useState<Set<number>>(new Set());
  const [padStates, setPadStates] = useState<PadState[]>(Array(PAD_COUNT).fill(null));
  const [baseVoices, setBaseVoices] = useState<16 | 32 | 64>(16);
  const [allowInfiniteVoices, setAllowInfiniteVoices] = useState(true);
  const [retrigger, setRetrigger] = useState(true);
  const [masterGain, setMasterGain] = useState(0);
  const [velSensDb, setVelSensDb] = useState(-60);
  const [padVolumes, setPadVolumes] = useState<number[]>(() => Array(PAD_COUNT).fill(1.0));
  const [padMonos, setPadMonos] = useState<boolean[]>(() => Array(PAD_COUNT).fill(false));
  const [padNormalizes, setPadNormalizes] = useState<boolean[]>(() => Array(PAD_COUNT).fill(false));
  const [connected, setConnected] = useState(false);
  const didInit = useRef(false);
  const guiVersion = import.meta.env.VITE_GUI_VERSION ?? "dev";

  useEffect(() => {
    if (didInit.current) return;
    didInit.current = true;

    window.onPluginMessage = (raw: unknown) => {
      const msg = raw as PluginMessage;
      if (msg.type === "State") {
        if (msg.cacheDirOverride !== undefined) setCacheDirOverride(msg.cacheDirOverride ?? null);
        if (msg.effectiveCacheDir != null) setEffectiveCacheDir(msg.effectiveCacheDir);
        if (msg.pluginVersion != null) setPluginVersion(msg.pluginVersion);
        if (msg.baseVoices != null) setBaseVoices(msg.baseVoices as 16 | 32 | 64);
        if (msg.allowInfiniteVoices != null) setAllowInfiniteVoices(msg.allowInfiniteVoices);
        if (msg.retrigger != null) setRetrigger(msg.retrigger);
        if (msg.masterGain != null) setMasterGain(msg.masterGain);
        if (msg.velSensDb != null) setVelSensDb(msg.velSensDb);
        if (msg.padVolumes != null) setPadVolumes(msg.padVolumes);
        if (msg.padMonos != null) setPadMonos(msg.padMonos);
        if (msg.padNormalizes != null) setPadNormalizes(msg.padNormalizes);
        setConnected(true);
      } else if (msg.type === "SampleLoaded") {
        setPadStates((prev) => {
          const next = [...prev];
          next[msg.padIndex] = { name: msg.name, loading: false };
          return next;
        });
      } else if (msg.type === "PadName") {
        setPadStates((prev) => {
          const next = [...prev];
          if (!next[msg.padIndex]) {
            next[msg.padIndex] = { name: msg.name, loading: true };
          }
          return next;
        });
      } else if (msg.type === "SampleError") {
        console.error(`Pad ${msg.padIndex} sample error:`, msg.message);
        setPadStates((prev) => {
          const next = [...prev];
          next[msg.padIndex] = null;
          return next;
        });
      } else if (msg.type === "PadCleared") {
        setPadStates((prev) => {
          const next = [...prev];
          next[msg.padIndex] = null;
          return next;
        });
      }
    };

    sendToPluginSafe({ type: "Init" });
    return () => { window.onPluginMessage = undefined; };
  }, []);

  const handleSetCustomDir = (path: string) => {
    sendToPluginSafe({ type: "SetCacheDir", path });
  };

  const handleRemoveCustomDir = () => {
    setPadStates(Array(PAD_COUNT).fill(null));
    sendToPluginSafe({ type: "ClearCacheDir" });
  };

  const handleSampleDrop = async (padIndex: number, file: File) => {
    const displayName = file.name.replace(/\.[^/.]+$/, "");
    setPadStates((prev) => {
      const next = [...prev];
      next[padIndex] = { name: displayName, loading: true };
      return next;
    });
    try {
      const decoded = await decodeAudioFile(file);
      sendToPluginSafe({
        type: "SaveSample",
        padIndex,
        name: displayName,
        sampleRate: decoded.sampleRate,
        channels: decoded.channels,
        frames: decoded.frames,
        dataBase64: decoded.dataBase64,
      });
    } catch (e) {
      console.error("Audio decode failed:", e);
      setPadStates((prev) => {
        const next = [...prev];
        next[padIndex] = null;
        return next;
      });
    }
  };

  const handleMasterGainChange = (gain: number) => {
    setMasterGain(gain);
    sendToPluginSafe({ type: "SetMasterGain", gainDb: gain });
  };

  const handleVelSensChange = (sensDb: number) => {
    setVelSensDb(sensDb);
    sendToPluginSafe({ type: "SetVelSens", sensDb });
  };

  const handleVoiceModeChange = (value: string) => {
    if (value === "inf") {
      handleAllowInfiniteVoicesChange(true);
    } else {
      const n = Number(value) as 16 | 32 | 64;
      if (allowInfiniteVoices) handleAllowInfiniteVoicesChange(false);
      handleBaseVoicesChange(n);
    }
  };

  const handleDeletePad = (padIndex: number) => {
    sendToPluginSafe({ type: "DeletePad", padIndex });
  };

  const handlePadNormalizeChange = (padIndex: number, normalize: boolean) => {
    setPadNormalizes((prev) => {
      const next = [...prev];
      next[padIndex] = normalize;
      return next;
    });
    sendToPluginSafe({ type: "SetPadNormalize", padIndex, normalize });
  };

  const handlePadMonoChange = (padIndex: number, mono: boolean) => {
    setPadMonos((prev) => {
      const next = [...prev];
      next[padIndex] = mono;
      return next;
    });
    sendToPluginSafe({ type: "SetPadMono", padIndex, mono });
  };

  const handlePadVolumeChange = (padIndex: number, volume: number) => {
    setPadVolumes((prev) => {
      const next = [...prev];
      next[padIndex] = volume;
      return next;
    });
    sendToPluginSafe({ type: "SetPadVolume", padIndex, volume });
  };

  const handleBaseVoicesChange = (v: 16 | 32 | 64) => {
    setBaseVoices(v);
    sendToPluginSafe({ type: "SetBaseVoices", voices: v });
  };

  const handleAllowInfiniteVoicesChange = (enabled: boolean) => {
    setAllowInfiniteVoices(enabled);
    sendToPluginSafe({ type: "SetAllowInfiniteVoices", enabled });
  };

  const handleRetriggerChange = (enabled: boolean) => {
    setRetrigger(enabled);
    sendToPluginSafe({ type: "SetRetrigger", enabled });
  };

  return (
    <div className="app">
      <header className="app-header">
        <div className="app-title">
          <span className="app-name">DISPATCH</span>
          <span className="app-tagline">drum rack</span>
        </div>

        <div className="app-controls">
          <div className="ctrl-group">
            <label className="ctrl-label">polyphony</label>
            <select
              className="ctrl-select"
              value={allowInfiniteVoices ? "inf" : String(baseVoices)}
              onChange={(e) => handleVoiceModeChange(e.target.value)}
              title="Maximum simultaneous voices. ∞ grows without limit."
            >
              <option value="16">16 voices</option>
              <option value="32">32 voices</option>
              <option value="64">64 voices</option>
              <option value="inf">∞ voices</option>
            </select>
          </div>

          <div className="ctrl-sep" />

          <div className="ctrl-group">
            <label className="ctrl-label">retrigger</label>
            <button
              className={`ctrl-toggle ${retrigger ? "ctrl-toggle--on" : ""}`}
              onClick={() => handleRetriggerChange(!retrigger)}
              title="When on, re-hitting a pad restarts the sample instead of layering a new voice"
            >
              {retrigger ? "on" : "off"}
            </button>
          </div>
        </div>
      </header>

      <CacheDirBar
        cacheDirOverride={cacheDirOverride}
        effectiveCacheDir={effectiveCacheDir}
        onSetCustomDir={handleSetCustomDir}
        onRemoveCustomDir={handleRemoveCustomDir}
      />

      <main className="app-main">
        <PadGrid activePads={activePads} padStates={padStates} onSampleDrop={handleSampleDrop} onDeletePad={handleDeletePad} />
      </main>

      <Mixer
        padStates={padStates}
        masterGain={masterGain}
        velSensDb={velSensDb}
        padVolumes={padVolumes}
        padMonos={padMonos}
        padNormalizes={padNormalizes}
        onMasterGainChange={handleMasterGainChange}
        onVelSensChange={handleVelSensChange}
        onVolumeChange={handlePadVolumeChange}
        onMonoChange={handlePadMonoChange}
        onNormalizeChange={handlePadNormalizeChange}
      />

      <footer className="app-footer">
        <span className={`conn-dot ${connected ? "conn-dot--on" : ""}`} />
        <span>{connected ? "Connected" : "Connecting..."}</span>
        <span className="footer-versions">plugin {pluginVersion ?? "—"} · gui {guiVersion}</span>
      </footer>
    </div>
  );
}
