import { useEffect, useRef, useState } from "react";

type PluginMessage =
  | {
      type: "State";
      cacheDir?: string | null;
      needsCacheDir?: boolean;
      pluginVersion?: string;
      maxVoices?: number;
      retrigger?: boolean;
      padVolumes?: number[];
      padMonos?: boolean[];
    }
  | { type: "SampleLoaded"; padIndex: number; name: string }
  | { type: "SampleError"; padIndex: number; message: string }
  | { type: "PadName"; padIndex: number; name: string };

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

  // Interleave channels into one Float32Array
  const interleaved = new Float32Array(length * numberOfChannels);
  for (let frame = 0; frame < length; frame++) {
    for (let ch = 0; ch < numberOfChannels; ch++) {
      interleaved[frame * numberOfChannels + ch] = channelArrays[ch][frame];
    }
  }

  // Base64 encode raw bytes
  const bytes = new Uint8Array(interleaved.buffer);
  let binary = "";
  const chunkSize = 0x8000;
  for (let i = 0; i < bytes.length; i += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunkSize));
  }

  return { sampleRate, channels: numberOfChannels, frames: length, dataBase64: btoa(binary) };
}

// ---------------------------------------------------------------------------
// Cache Dir Setup Screen
// ---------------------------------------------------------------------------

function CacheDirSetup({ onConfirm }: { onConfirm: (path: string) => void }) {
  const [input, setInput] = useState("");
  const handleSubmit = () => {
    const t = input.trim();
    if (t) onConfirm(t);
  };
  return (
    <div className="setup-overlay">
      <div className="setup-card">
        <div className="setup-icon">📁</div>
        <h2 className="setup-title">Choose a Cache Directory</h2>
        <p className="setup-desc">
          Dispatch needs a folder to store cached samples and project data.
        </p>
        <input
          className="setup-input"
          type="text"
          placeholder="e.g. C:\Users\you\Music\dispatch-cache"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSubmit()}
          autoFocus
          spellCheck={false}
        />
        <button className="setup-btn" onClick={handleSubmit} disabled={!input.trim()}>
          Set Cache Directory
        </button>
        <p className="setup-hint">Tip: paste a full path. The folder will be created if needed.</p>
      </div>
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
}: {
  activePads: Set<number>;
  padStates: PadState[];
  onSampleDrop: (padIndex: number, file: File) => void;
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
  padVolumes,
  padMonos,
  onVolumeChange,
  onMonoChange,
}: {
  padStates: PadState[];
  padVolumes: number[];
  padMonos: boolean[];
  onVolumeChange: (padIndex: number, volume: number) => void;
  onMonoChange: (padIndex: number, mono: boolean) => void;
}) {
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
          <button
            className={`mixer-mono-btn${padMonos[i] ? " mixer-mono-btn--on" : ""}`}
            onClick={() => onMonoChange(i, !padMonos[i])}
            title="Force mono"
          >
            M
          </button>
          <span className="mixer-val">
            {padVolumes[i] === 1.0
              ? "0dB"
              : padVolumes[i] === 0
              ? "−∞"
              : `${((padVolumes[i] - 1) * 100).toFixed(0)}%`}
          </span>
        </div>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main App
// ---------------------------------------------------------------------------

export default function App() {
  const [cacheDir, setCacheDir] = useState<string | null>(null);
  const [needsCacheDir, setNeedsCacheDir] = useState(false);
  const [pluginVersion, setPluginVersion] = useState<string | null>(null);
  const [activePads] = useState<Set<number>>(new Set());
  const [padStates, setPadStates] = useState<PadState[]>(Array(PAD_COUNT).fill(null));
  const [maxVoices, setMaxVoices] = useState<0 | 16 | 32 | 64>(16);
  const [retrigger, setRetrigger] = useState(true);
  const [padVolumes, setPadVolumes] = useState<number[]>(() => Array(PAD_COUNT).fill(1.0));
  const [padMonos, setPadMonos] = useState<boolean[]>(() => Array(PAD_COUNT).fill(false));
  const [connected, setConnected] = useState(false);
  const didInit = useRef(false);
  const guiVersion = import.meta.env.VITE_GUI_VERSION ?? "dev";

  useEffect(() => {
    if (didInit.current) return;
    didInit.current = true;

    window.onPluginMessage = (raw: unknown) => {
      const msg = raw as PluginMessage;
      if (msg.type === "State") {
        if (msg.cacheDir !== undefined) setCacheDir(msg.cacheDir);
        if (msg.needsCacheDir !== undefined) setNeedsCacheDir(msg.needsCacheDir);
        if (msg.pluginVersion != null) setPluginVersion(msg.pluginVersion);
        if (msg.maxVoices != null) setMaxVoices(msg.maxVoices as 0 | 16 | 32 | 64);
        if (msg.retrigger != null) setRetrigger(msg.retrigger);
        if (msg.padVolumes != null) setPadVolumes(msg.padVolumes);
        if (msg.padMonos != null) setPadMonos(msg.padMonos);
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
      }
    };

    sendToPluginSafe({ type: "Init" });
    return () => { window.onPluginMessage = undefined; };
  }, []);

  const handleCacheDirConfirm = (path: string) => sendToPluginSafe({ type: "SetCacheDir", path });
  const handleClearCacheDir = () => sendToPluginSafe({ type: "ClearCacheDir" });

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

  const handlePolyphonyChange = (v: 0 | 16 | 32 | 64) => {
    setMaxVoices(v);
    sendToPluginSafe({ type: "SetPolyphony", voices: v });
  };

  const handleRetriggerChange = (enabled: boolean) => {
    setRetrigger(enabled);
    sendToPluginSafe({ type: "SetRetrigger", enabled });
  };

  return (
    <div className="app">
      {needsCacheDir && <CacheDirSetup onConfirm={handleCacheDirConfirm} />}

      <header className="app-header">
        <div className="app-title">
          <span className="app-name">DISPATCH</span>
          <span className="app-tagline">drum rack</span>
        </div>

        <div className="app-controls">
          <label className="ctrl-label">voices</label>
          <select
            className="ctrl-select"
            value={maxVoices}
            onChange={(e) => handlePolyphonyChange(Number(e.target.value) as 0 | 16 | 32 | 64)}
          >
            <option value={16}>16</option>
            <option value={32}>32</option>
            <option value={64}>64</option>
            <option value={0}>∞</option>
          </select>

          <label className="ctrl-label">retrigger</label>
          <button
            className={`ctrl-toggle ${retrigger ? "ctrl-toggle--on" : ""}`}
            onClick={() => handleRetriggerChange(!retrigger)}
          >
            {retrigger ? "on" : "off"}
          </button>
        </div>

        <div className="cache-status">
          {cacheDir ? (
            <>
              <span className="cache-icon">💾</span>
              <span className="cache-path" title={cacheDir}>{cacheDir}</span>
              <button className="cache-clear-btn" onClick={handleClearCacheDir} title="Change cache directory">✕</button>
            </>
          ) : (
            <span className="cache-missing">No cache directory set</span>
          )}
        </div>
      </header>

      <main className="app-main">
        <PadGrid activePads={activePads} padStates={padStates} onSampleDrop={handleSampleDrop} />
      </main>

      <Mixer padStates={padStates} padVolumes={padVolumes} padMonos={padMonos} onVolumeChange={handlePadVolumeChange} onMonoChange={handlePadMonoChange} />

      <footer className="app-footer">
        <span className={`conn-dot ${connected ? "conn-dot--on" : ""}`} />
        <span>{connected ? "Connected" : "Connecting..."}</span>
        <span className="footer-versions">plugin {pluginVersion ?? "—"} · gui {guiVersion}</span>
      </footer>
    </div>
  );
}
