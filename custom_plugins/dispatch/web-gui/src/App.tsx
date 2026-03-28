import { useEffect, useRef, useState } from "react";

type PluginMessage =
  | {
      type: "State";
      cacheDir: string | null;
      needsCacheDir: boolean;
      pluginVersion?: string;
    };

const sendToPluginSafe = (payload: unknown) => {
  if (typeof window.sendToPlugin === "function") {
    window.sendToPlugin(payload);
  } else {
    console.info("sendToPlugin not available", payload);
  }
};

// 16 pads in a 4x4 grid — MIDI notes 36–51 (General MIDI drum standard)
const PAD_COUNT = 16;
const PAD_MIDI_BASE = 36;

const PAD_LABELS = [
  "Kick",    "Snare",   "Clap",    "Rim",
  "Hi-Hat",  "Open HH", "Ride",    "Crash",
  "Tom 1",   "Tom 2",   "Tom 3",   "Tom 4",
  "Perc 1",  "Perc 2",  "Perc 3",  "Perc 4",
];

// ---------------------------------------------------------------------------
// Cache Dir Setup Screen
// ---------------------------------------------------------------------------

function CacheDirSetup({ onConfirm }: { onConfirm: (path: string) => void }) {
  const [input, setInput] = useState("");

  const handleSubmit = () => {
    const trimmed = input.trim();
    if (trimmed.length > 0) {
      onConfirm(trimmed);
    }
  };

  return (
    <div className="setup-overlay">
      <div className="setup-card">
        <div className="setup-icon">📁</div>
        <h2 className="setup-title">Choose a Cache Directory</h2>
        <p className="setup-desc">
          Dispatch needs a folder to store cached samples and project data.
          This is usually your project folder or a dedicated samples directory.
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
        <button
          className="setup-btn"
          onClick={handleSubmit}
          disabled={input.trim().length === 0}
        >
          Set Cache Directory
        </button>
        <p className="setup-hint">
          Tip: paste a full path. The folder will be created if it doesn't exist.
        </p>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Pad Grid
// ---------------------------------------------------------------------------

function PadGrid({ activePads }: { activePads: Set<number> }) {
  return (
    <div className="pad-grid">
      {Array.from({ length: PAD_COUNT }, (_, i) => {
        const midiNote = PAD_MIDI_BASE + i;
        const isActive = activePads.has(i);
        return (
          <div
            key={i}
            className={`pad ${isActive ? "pad--active" : ""}`}
            title={`MIDI ${midiNote}`}
          >
            <div className="pad-label">{PAD_LABELS[i]}</div>
            <div className="pad-note">C{Math.floor(midiNote / 12) - 1}{String.fromCharCode(65)}</div>
            <div className="pad-sample pad-sample--empty">Drop sample</div>
          </div>
        );
      })}
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
  const [connected, setConnected] = useState(false);
  const didInit = useRef(false);
  const guiVersion = import.meta.env.VITE_GUI_VERSION ?? "dev";

  useEffect(() => {
    if (didInit.current) return;
    didInit.current = true;

    window.onPluginMessage = (raw: unknown) => {
      const message = raw as PluginMessage;
      if (message.type === "State") {
        setCacheDir(message.cacheDir);
        setNeedsCacheDir(message.needsCacheDir);
        if (message.pluginVersion) setPluginVersion(message.pluginVersion);
        setConnected(true);
      }
    };

    sendToPluginSafe({ type: "Init" });

    return () => {
      window.onPluginMessage = undefined;
    };
  }, []);

  const handleCacheDirConfirm = (path: string) => {
    sendToPluginSafe({ type: "SetCacheDir", path });
  };

  const handleClearCacheDir = () => {
    sendToPluginSafe({ type: "ClearCacheDir" });
  };

  return (
    <div className="app">
      {needsCacheDir && <CacheDirSetup onConfirm={handleCacheDirConfirm} />}

      <header className="app-header">
        <div className="app-title">
          <span className="app-name">DISPATCH</span>
          <span className="app-tagline">drum rack</span>
        </div>
        <div className="cache-status">
          {cacheDir ? (
            <>
              <span className="cache-icon">💾</span>
              <span className="cache-path" title={cacheDir}>{cacheDir}</span>
              <button className="cache-clear-btn" onClick={handleClearCacheDir} title="Change cache directory">
                ✕
              </button>
            </>
          ) : (
            <span className="cache-missing">No cache directory set</span>
          )}
        </div>
      </header>

      <main className="app-main">
        <PadGrid activePads={activePads} />
      </main>

      <footer className="app-footer">
        <span className={`conn-dot ${connected ? "conn-dot--on" : ""}`} />
        <span>{connected ? "Connected" : "Connecting..."}</span>
        <span className="footer-versions">
          plugin {pluginVersion ?? "—"} · gui {guiVersion}
        </span>
      </footer>
    </div>
  );
}
