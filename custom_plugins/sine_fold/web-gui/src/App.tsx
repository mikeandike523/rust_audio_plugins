import { useEffect, useRef, useState } from "react";

type MeterValues = {
  l: number;
  r: number;
};

type PluginMessage =
  | {
      type: "ParamChange";
      fold?: number;
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
  const [fold, setFold] = useState(1);
  const [gain, setGain] = useState(0);
  const [status, setStatus] = useState("Waiting for plugin...");
  const [meterInput, setMeterInput] = useState<MeterValues>({ l: 0, r: 0 });
  const [meterOutput, setMeterOutput] = useState<MeterValues>({ l: 0, r: 0 });
  const [pluginVersion, setPluginVersion] = useState<string | null>(null);
  const [loadedFrom] = useState(() => window.location.href);
  const [isOffline, setIsOffline] = useState(() => !navigator.onLine);
  const didInit = useRef(false);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const guiVersion = import.meta.env.VITE_GUI_VERSION ?? "dev";

  useEffect(() => {
    if (didInit.current) {
      return;
    }
    didInit.current = true;

    window.onPluginMessage = (message: PluginMessage) => {
      if (message.type === "ParamChange") {
        if (typeof message.fold === "number") {
          setFold(clamp(message.fold, 0, 50));
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

  const handleFoldChange = (value: number) => {
    const clamped = clamp(value, 0, 50);
    setFold(clamped);
    sendToPluginSafe({ type: "SetFold", value: clamped });
  };

  const handleGainChange = (value: number) => {
    const clamped = clamp(value, -24, 24);
    setGain(clamped);
    sendToPluginSafe({ type: "SetGain", value: clamped });
  };

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return;
    }

    const draw = () => {
      const context = canvas.getContext("2d");
      if (!context) {
        return;
      }

      const rect = canvas.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      const width = Math.max(1, Math.floor(rect.width * dpr));
      const height = Math.max(1, Math.floor(rect.height * dpr));

      if (canvas.width !== width || canvas.height !== height) {
        canvas.width = width;
        canvas.height = height;
      }

      context.setTransform(dpr, 0, 0, dpr, 0, 0);

      const style = getComputedStyle(document.documentElement);
      const gridColor = style.getPropertyValue("--meter-bg").trim() || "#efe6dc";
      const axisColor = style.getPropertyValue("--muted").trim() || "#9a9389";
      const curveColor = style.getPropertyValue("--accent-dark").trim() || "#b85d2b";
      const panelColor = style.getPropertyValue("--panel").trim() || "#ffffff";

      const cssWidth = rect.width;
      const cssHeight = rect.height;
      const padding = 18;
      const plotWidth = Math.max(1, cssWidth - padding * 2);
      const plotHeight = Math.max(1, cssHeight - padding * 2);

      const yMax = 1;

      const xToPixel = (value: number) =>
        padding + ((value + 1) / 2) * plotWidth;
      const yToPixel = (value: number) =>
        padding + ((yMax - value) / (2 * yMax)) * plotHeight;

      context.clearRect(0, 0, cssWidth, cssHeight);
      context.fillStyle = panelColor;
      context.fillRect(0, 0, cssWidth, cssHeight);

      context.strokeStyle = gridColor;
      context.lineWidth = 1;
      context.beginPath();
      for (let i = 1; i < 4; i += 1) {
        const x = padding + (plotWidth / 4) * i;
        context.moveTo(x, padding);
        context.lineTo(x, padding + plotHeight);
      }
      for (let i = 1; i < 4; i += 1) {
        const y = padding + (plotHeight / 4) * i;
        context.moveTo(padding, y);
        context.lineTo(padding + plotWidth, y);
      }
      context.stroke();

      context.strokeStyle = axisColor;
      context.lineWidth = 1.5;
      context.beginPath();
      context.moveTo(xToPixel(-1), yToPixel(0));
      context.lineTo(xToPixel(1), yToPixel(0));
      context.moveTo(xToPixel(0), yToPixel(-yMax));
      context.lineTo(xToPixel(0), yToPixel(yMax));
      context.stroke();

      context.strokeStyle = curveColor;
      context.lineWidth = 2;
      context.beginPath();

      const step = 1 / 200;
      let isFirst = true;
      for (let x = -1; x <= 1.00001; x += step) {
        const y = Math.sin(fold * (Math.PI / 2) * x);
        const px = xToPixel(x);
        const py = yToPixel(y);
        if (isFirst) {
          context.moveTo(px, py);
          isFirst = false;
        } else {
          context.lineTo(px, py);
        }
      }
      context.stroke();
    };

    draw();
    const handleResize = () => draw();
    window.addEventListener("resize", handleResize);

    return () => {
      window.removeEventListener("resize", handleResize);
    };
  }, [fold]);

  return (
    <div className="panel">
      <header>
        <div>
          <h1>Sine Fold</h1>
          <div className="subtitle">Sine-fold + Gain</div>
        </div>
        <div className="source">
          <div className="source-label">Loaded From</div>
          <div className="source-value" title={loadedFrom}>
            {loadedFrom}
          </div>
        </div>
      </header>

      <section className="visualization">
        <div className="visualization-header">
          <span>Transfer Curve</span>
          <span>v_out = sin(k * pi/2 * v_in)</span>
        </div>
        <div className="visualization-subtitle">Gain applies after folding.</div>
        <canvas ref={canvasRef} className="curve-canvas" />
      </section>

      <section className="controls">
        <div className="control">
          <label htmlFor="fold">Fold (k)</label>
          <input
            id="fold"
            type="range"
            min="0"
            max="50"
            step="0.1"
            value={fold}
            onChange={(event) => handleFoldChange(Number(event.target.value))}
          />
          <div className="value">{fold.toFixed(2)}</div>
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
      {isOffline && (
        <div className="offline-banner">
          Offline — loaded from PWA cache · {loadedFrom}
        </div>
      )}
    </div>
  );
}
