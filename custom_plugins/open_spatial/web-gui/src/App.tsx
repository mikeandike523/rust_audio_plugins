import { useEffect, useMemo, useRef, useState } from "react";

type CoordinateMode = "spherical" | "cylindrical";

type MeterValues = {
  l: number;
  r: number;
};

type PluginMessage =
  | {
      type: "State";
      coordinateMode: CoordinateMode;
      azimuth?: number;
      elevation?: number;
      distance?: number;
      radius?: number;
      height?: number;
      sourceYaw?: number;
      directivity?: number;
      outputGain?: number;
      pluginVersion?: string;
      rendererId?: string;
      cachePath?: string;
      hrtfPath?: string;
      hrtfUrl?: string;
      initStage?: string;
      initMessage?: string;
      initProgress?: number | null;
      downloadedBytes?: number | null;
      totalBytes?: number | null;
      hrtfReady?: boolean;
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

const fmt = (value: number, digits = 1) => value.toFixed(digits);

const polarToCartesian = (azimuth: number, radius: number) => {
  const radians = (azimuth * Math.PI) / 180;
  return {
    x: Math.cos(radians) * radius,
    y: Math.sin(radians) * radius,
  };
};

const formatBytes = (value?: number | null) => {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return "unknown";
  }
  if (value < 1024 * 1024) {
    return `${(value / 1024).toFixed(1)} KB`;
  }
  return `${(value / (1024 * 1024)).toFixed(2)} MB`;
};

function ControlRow(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  unit: string;
  digits?: number;
  onChange: (value: number) => void;
}) {
  const { label, value, min, max, step, unit, digits = 1, onChange } = props;

  return (
    <label className="control-row">
      <div className="control-meta">
        <span>{label}</span>
        <strong>
          {fmt(value, digits)}
          {unit}
        </strong>
      </div>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
      />
    </label>
  );
}

export default function App() {
  const [coordinateMode, setCoordinateMode] = useState<CoordinateMode>("spherical");
  const [azimuth, setAzimuth] = useState(30);
  const [elevation, setElevation] = useState(0);
  const [distance, setDistance] = useState(1.5);
  const [radius, setRadius] = useState(1.5);
  const [height, setHeight] = useState(0);
  const [sourceYaw, setSourceYaw] = useState(180);
  const [directivity, setDirectivity] = useState(0.65);
  const [outputGain, setOutputGain] = useState(-3);
  const [status, setStatus] = useState("Waiting for plugin...");
  const [cachePath, setCachePath] = useState("");
  const [hrtfPath, setHrtfPath] = useState("");
  const [hrtfUrl, setHrtfUrl] = useState("");
  const [initStage, setInitStage] = useState("idle");
  const [initMessage, setInitMessage] = useState("Waiting for initialization");
  const [initProgress, setInitProgress] = useState<number | null>(null);
  const [downloadedBytes, setDownloadedBytes] = useState<number | null>(null);
  const [totalBytes, setTotalBytes] = useState<number | null>(null);
  const [hrtfReady, setHrtfReady] = useState(false);
  const [pluginVersion, setPluginVersion] = useState<string | null>(null);
  const [rendererId, setRendererId] = useState("sofa-runtime-fetch-v1");
  const [meterInput, setMeterInput] = useState<MeterValues>({ l: 0, r: 0 });
  const [meterOutput, setMeterOutput] = useState<MeterValues>({ l: 0, r: 0 });
  const [loadedFrom] = useState(() => window.location.href);
  const didInit = useRef(false);
  const guiVersion = import.meta.env.VITE_GUI_VERSION ?? "dev";

  useEffect(() => {
    if (didInit.current) {
      return;
    }
    didInit.current = true;

    window.onPluginMessage = (message: PluginMessage) => {
      if (message.type === "State") {
        setCoordinateMode(message.coordinateMode ?? "spherical");
        setAzimuth(message.azimuth ?? 0);
        setElevation(message.elevation ?? 0);
        setDistance(message.distance ?? 1);
        setRadius(message.radius ?? 1);
        setHeight(message.height ?? 0);
        setSourceYaw(message.sourceYaw ?? 180);
        setDirectivity(message.directivity ?? 0);
        setOutputGain(message.outputGain ?? 0);
        setPluginVersion(message.pluginVersion ?? null);
        setRendererId(message.rendererId ?? "sofa-runtime-fetch-v1");
        setCachePath(message.cachePath ?? "");
        setHrtfPath(message.hrtfPath ?? "");
        setHrtfUrl(message.hrtfUrl ?? "");
        setInitStage(message.initStage ?? "idle");
        setInitMessage(message.initMessage ?? "Waiting for initialization");
        setInitProgress(
          typeof message.initProgress === "number" ? clamp(message.initProgress, 0, 1) : null,
        );
        setDownloadedBytes(message.downloadedBytes ?? null);
        setTotalBytes(message.totalBytes ?? null);
        setHrtfReady(Boolean(message.hrtfReady));
        setStatus(message.hrtfReady ? "Measured HRTF ready" : "Initializing HRTF...");
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
      window.onPluginMessage = undefined;
    };
  }, []);

  const displayDistance = coordinateMode === "spherical" ? distance : radius;
  const sourcePoint = useMemo(
    () => polarToCartesian(azimuth, clamp(displayDistance / 30, 0.1, 1) * 118),
    [azimuth, displayDistance],
  );
  const directivityLabel = `${Math.round(directivity * 100)}%`;
  const visibleCoordinates =
    coordinateMode === "spherical"
      ? `az ${fmt(azimuth)} deg  |  el ${fmt(elevation)} deg  |  r ${fmt(distance, 2)} m`
      : `az ${fmt(azimuth)} deg  |  radius ${fmt(radius, 2)} m  |  h ${fmt(height, 2)} m`;
  const progressPercent =
    typeof initProgress === "number" ? `${Math.round(initProgress * 100)}%` : "Pending";
  const downloadSizeLine =
    downloadedBytes != null || totalBytes != null
      ? `${formatBytes(downloadedBytes)} / ${formatBytes(totalBytes)}`
      : "Waiting for transfer";

  return (
    <div className="shell">
      <section className="panel hero">
        <div className="hero-copy">
          <div className="eyebrow">Open Spatial</div>
          <h1>Runtime-downloaded HRTF spatializer.</h1>
          <p>
            The plugin keeps only URLs in code, downloads the HRTF on first use, validates the
            cache on later loads, and stays silent until the measured renderer is ready.
          </p>
        </div>

        <div className="hero-meta">
          <div className="meta-card">
            <span>Renderer</span>
            <strong>{rendererId}</strong>
          </div>
          <div className="meta-card">
            <span>Init Stage</span>
            <strong>{initStage}</strong>
          </div>
          <div className="meta-card">
            <span>Audio</span>
            <strong>{status}</strong>
          </div>
        </div>
      </section>

      <section className="panel layout-grid">
        <div className="stage-card">
          <div className="section-label">Scene</div>
          <div className="stage-copy">
            <strong>{visibleCoordinates}</strong>
            <span>Positive azimuth is to the listener&apos;s right. Audio stays silent until the HRTF cache is ready.</span>
          </div>

          <div className="stage">
            {!hrtfReady ? (
              <div className="init-overlay">
                <div className="init-chip">{initStage}</div>
                <strong>{initMessage}</strong>
                <div className="progress-track">
                  <div
                    className={`progress-fill ${typeof initProgress === "number" ? "" : "indeterminate"}`}
                    style={{
                      width: typeof initProgress === "number" ? `${initProgress * 100}%` : "45%",
                    }}
                  ></div>
                </div>
                <div className="progress-meta">
                  <span>{progressPercent}</span>
                  <span>{downloadSizeLine}</span>
                </div>
              </div>
            ) : null}

            <div className="grid-ring ring-1"></div>
            <div className="grid-ring ring-2"></div>
            <div className="grid-ring ring-3"></div>
            <div className="axis axis-x"></div>
            <div className="axis axis-y"></div>
            <div className="listener">
              <div className="listener-head"></div>
              <span>Listener</span>
            </div>

            <div
              className="source-marker"
              style={{
                left: `calc(50% + ${sourcePoint.x}px)`,
                top: `calc(50% + ${sourcePoint.y}px)`,
              }}
            >
              <div className="source-dot"></div>
              <div
                className="source-heading"
                style={{
                  transform: `translate(-10px, -10px) rotate(${sourceYaw}deg)`,
                  opacity: 0.35 + directivity * 0.65,
                }}
              ></div>
              <span>Source</span>
            </div>
          </div>

          <div className="stage-footer">
            <div>
              <span>Directivity</span>
              <strong>{directivityLabel}</strong>
            </div>
            <div>
              <span>Output</span>
              <strong>{fmt(outputGain)} dB</strong>
            </div>
          </div>
        </div>

        <div className="controls-card">
          <div className="section-label">Controls</div>

          <div className="mode-switch">
            <button
              className={coordinateMode === "spherical" ? "active" : ""}
              onClick={() => sendToPluginSafe({ type: "SetCoordinateMode", value: "spherical" })}
            >
              Spherical
            </button>
            <button
              className={coordinateMode === "cylindrical" ? "active" : ""}
              onClick={() => sendToPluginSafe({ type: "SetCoordinateMode", value: "cylindrical" })}
            >
              Cylindrical
            </button>
          </div>

          <div className="control-group">
            <ControlRow
              label="Azimuth"
              value={azimuth}
              min={-180}
              max={180}
              step={0.1}
              unit=" deg"
              onChange={(value) => {
                setAzimuth(value);
                sendToPluginSafe({ type: "SetAzimuth", value });
              }}
            />

            {coordinateMode === "spherical" ? (
              <>
                <ControlRow
                  label="Elevation"
                  value={elevation}
                  min={-90}
                  max={90}
                  step={0.1}
                  unit=" deg"
                  onChange={(value) => {
                    setElevation(value);
                    sendToPluginSafe({ type: "SetElevation", value });
                  }}
                />
                <ControlRow
                  label="Distance"
                  value={distance}
                  min={1}
                  max={30}
                  step={0.01}
                  unit=" m"
                  digits={2}
                  onChange={(value) => {
                    setDistance(value);
                    sendToPluginSafe({ type: "SetDistance", value });
                  }}
                />
              </>
            ) : (
              <>
                <ControlRow
                  label="Radius"
                  value={radius}
                  min={1}
                  max={30}
                  step={0.01}
                  unit=" m"
                  digits={2}
                  onChange={(value) => {
                    setRadius(value);
                    sendToPluginSafe({ type: "SetRadius", value });
                  }}
                />
                <ControlRow
                  label="Height"
                  value={height}
                  min={-10}
                  max={10}
                  step={0.01}
                  unit=" m"
                  digits={2}
                  onChange={(value) => {
                    setHeight(value);
                    sendToPluginSafe({ type: "SetHeight", value });
                  }}
                />
              </>
            )}

            <ControlRow
              label="Source Yaw"
              value={sourceYaw}
              min={-180}
              max={180}
              step={0.1}
              unit=" deg"
              onChange={(value) => {
                setSourceYaw(value);
                sendToPluginSafe({ type: "SetSourceYaw", value });
              }}
            />
            <ControlRow
              label="Directivity"
              value={directivity}
              min={0}
              max={1}
              step={0.01}
              unit=""
              digits={2}
              onChange={(value) => {
                setDirectivity(value);
                sendToPluginSafe({ type: "SetDirectivity", value });
              }}
            />
            <ControlRow
              label="Output Gain"
              value={outputGain}
              min={-24}
              max={12}
              step={0.1}
              unit=" dB"
              onChange={(value) => {
                setOutputGain(value);
                sendToPluginSafe({ type: "SetOutputGain", value });
              }}
            />
          </div>
        </div>
      </section>

      <section className="panel bottom-grid">
        <div className="meters-card">
          <div className="section-label">Meters</div>
          <div className="meters-grid">
            <div className="meter-block">
              <span>Input L</span>
              <div className="meter-rail">
                <div className="meter-fill" style={{ width: `${meterInput.l * 100}%` }}></div>
              </div>
            </div>
            <div className="meter-block">
              <span>Input R</span>
              <div className="meter-rail">
                <div className="meter-fill" style={{ width: `${meterInput.r * 100}%` }}></div>
              </div>
            </div>
            <div className="meter-block">
              <span>Output L</span>
              <div className="meter-rail">
                <div className="meter-fill hot" style={{ width: `${meterOutput.l * 100}%` }}></div>
              </div>
            </div>
            <div className="meter-block">
              <span>Output R</span>
              <div className="meter-rail">
                <div className="meter-fill hot" style={{ width: `${meterOutput.r * 100}%` }}></div>
              </div>
            </div>
          </div>
        </div>

        <div className="asset-card">
          <div className="section-label">Runtime Fetch</div>
          <div className="asset-state">{initMessage}</div>
          <div className="asset-paths">
            <div>
              <span>Source URL</span>
              <strong title={hrtfUrl}>{hrtfUrl || "not set"}</strong>
            </div>
            <div>
              <span>Cache Root</span>
              <strong title={cachePath}>{cachePath || "not set"}</strong>
            </div>
            <div>
              <span>Cached HRTF</span>
              <strong title={hrtfPath}>{hrtfPath || "not set"}</strong>
            </div>
            <div>
              <span>Loaded From</span>
              <strong title={loadedFrom}>{loadedFrom}</strong>
            </div>
          </div>
          <button className="validate-button" onClick={() => sendToPluginSafe({ type: "ValidateCache" })}>
            Revalidate Cache
          </button>
        </div>
      </section>

      <footer className="footer">
        <span>plugin {pluginVersion ?? "unknown"}</span>
        <span>gui {guiVersion}</span>
      </footer>
    </div>
  );
}
