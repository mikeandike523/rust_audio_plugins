import { useEffect, useRef, useState } from "react";

type MeterValues = {
  l: number;
  r: number;
};

type PluginMessage =
  | {
      type: "ParamChange";
      saturation?: number;
      gain?: number;
      pluginVersion?: string;
    }
  | {
      type: "Meter";
      input?: Partial<MeterValues>;
      output?: Partial<MeterValues>;
    };

const clamp = (value: number, min: number, max: number) =>
  Math.min(max, Math.max(min, value));

const sendToPluginSafe = (payload: unknown) => {
  if (typeof window.sendToPlugin === "function") {
    window.sendToPlugin(payload);
  } else {
    console.info("sendToPlugin missing", payload);
  }
};

export default function App() {
  const [saturation, setSaturation] = useState(1);
  const [gain, setGain] = useState(0);
  const [status, setStatus] = useState("Waiting for plugin...");
  const [meterInput, setMeterInput] = useState<MeterValues>({ l: 0, r: 0 });
  const [meterOutput, setMeterOutput] = useState<MeterValues>({ l: 0, r: 0 });
  const [pluginVersion, setPluginVersion] = useState<string | null>(null);
  const [loadedFrom] = useState(() => window.location.href);
  const didInit = useRef(false);
  const guiVersion = import.meta.env.VITE_GUI_VERSION ?? "dev";

  useEffect(() => {
    if (didInit.current) {
      return;
    }
    didInit.current = true;

    window.onPluginMessage = (message: PluginMessage) => {
      if (message.type === "ParamChange") {
        if (typeof message.saturation === "number") {
          setSaturation(clamp(message.saturation, 0, 10));
        }
        if (typeof message.gain === "number") {
          setGain(clamp(message.gain, -24, 24));
        }
        if (typeof message.pluginVersion === "string") {
          setPluginVersion(message.pluginVersion);
        }
        setStatus("Connected");
      }

      if (message.type === "Meter") {
        setMeterInput({
          l: clamp(message.input?.l ?? 0, 0, 1),
          r: clamp(message.input?.r ?? 0, 0, 1),
        });
        setMeterOutput({
          l: clamp(message.output?.l ?? 0, 0, 1),
          r: clamp(message.output?.r ?? 0, 0, 1),
        });
      }
    };

    sendToPluginSafe({ type: "Init" });

    return () => {
      if (window.onPluginMessage) {
        window.onPluginMessage = undefined;
      }
    };
  }, []);

  const handleSaturationChange = (value: number) => {
    const clamped = clamp(value, 0, 10);
    setSaturation(clamped);
    sendToPluginSafe({ type: "SetSaturation", value: clamped });
  };

  const handleGainChange = (value: number) => {
    const clamped = clamp(value, -24, 24);
    setGain(clamped);
    sendToPluginSafe({ type: "SetGain", value: clamped });
  };

  return (
    <div className="panel">
      <header>
        <div>
          <h1>Tunable Sampler</h1>
          <div className="subtitle">Saturation + Gain</div>
        </div>
        <div className="source">
          <div className="source-label">Loaded From</div>
          <div className="source-value" title={loadedFrom}>
            {loadedFrom}
          </div>
        </div>
      </header>

      <section className="controls">
        <div className="control">
          <label htmlFor="saturation">Saturation</label>
          <input
            id="saturation"
            type="range"
            min="0"
            max="10"
            step="0.01"
            value={saturation}
            onChange={(event) => handleSaturationChange(Number(event.target.value))}
          />
          <div className="value">{saturation.toFixed(2)}x</div>
        </div>
        <div className="control">
          <label htmlFor="gain">Gain</label>
          <input
            id="gain"
            type="range"
            min="-24"
            max="24"
            step="0.1"
            value={gain}
            onChange={(event) => handleGainChange(Number(event.target.value))}
          />
          <div className="value">{gain.toFixed(1)} dB</div>
        </div>
      </section>

      <section className="meters">
        <div className="meter-group">
          <div className="meter-label">Input Level</div>
          <div className="meter-row">
            <div className="meter">
              <span>L</span>
              <div
                className="meter-fill"
                style={{ transform: `scaleY(${meterInput.l})` }}
              ></div>
            </div>
            <div className="meter">
              <span>R</span>
              <div
                className="meter-fill"
                style={{ transform: `scaleY(${meterInput.r})` }}
              ></div>
            </div>
          </div>
        </div>
        <div className="meter-group">
          <div className="meter-label">Output Level</div>
          <div className="meter-row">
            <div className="meter">
              <span>L</span>
              <div
                className="meter-fill"
                style={{ transform: `scaleY(${meterOutput.l})` }}
              ></div>
            </div>
            <div className="meter">
              <span>R</span>
              <div
                className="meter-fill"
                style={{ transform: `scaleY(${meterOutput.r})` }}
              ></div>
            </div>
          </div>
        </div>
      </section>

      <div className="version-meta">
        <div>plugin-version: {pluginVersion ?? "unknown"}</div>
        <div>gui-version: {guiVersion}</div>
      </div>

      <div className="footer">{status}</div>
    </div>
  );
}
