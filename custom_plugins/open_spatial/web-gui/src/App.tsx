import { useEffect, useMemo, useRef, useState } from "react";
import { debugLog } from "./debugLog";

type MeterValues = {
  l: number;
  r: number;
};

type PluginMessage =
  | {
      type: "State";
      azimuth?: number;
      elevation?: number;
      distance?: number;
      sourceYaw?: number;
      alwaysTowardsHead?: boolean;
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

const isPluginMessage = (message: unknown): message is PluginMessage => {
  if (!message || typeof message !== "object") {
    return false;
  }

  const candidate = message as { type?: unknown };
  return candidate.type === "State" || candidate.type === "Meter";
};

const clamp = (value: number, min: number, max: number) =>
  Math.min(max, Math.max(min, value));

const normalizeAngle = (value: number) => {
  let normalized = value % 360;
  if (normalized > 180) {
    normalized -= 360;
  }
  if (normalized <= -180) {
    normalized += 360;
  }
  return normalized;
};

const getAutoSourceYaw = (azimuth: number) => normalizeAngle(azimuth + 180);

const sendToPluginSafe = (payload: unknown) => {
  if (typeof window.sendToPlugin === "function") {
    debugLog("send_to_plugin", { payload });
    window.sendToPlugin(payload);
  } else {
    debugLog("send_to_plugin_missing", { payload });
    console.info("sendToPlugin missing", payload);
  }
};

const fmt = (value: number, digits = 1) => value.toFixed(digits);

const polarToCartesianTopView = (azimuth: number, radius: number) => {
  const radians = (azimuth * Math.PI) / 180;
  return {
    x: Math.sin(radians) * radius,
    y: -Math.cos(radians) * radius,
  };
};

function OrientationBadge(props: {
  azimuth: number;
  sourceYaw: number;
  directivity: number;
}) {
  const { azimuth, sourceYaw, directivity } = props;
  const center = 62;
  const sourceOffset = polarToCartesianTopView(azimuth, 30);
  const sourceX = center + sourceOffset.x;
  const sourceY = center + sourceOffset.y;

  return (
    <div className="orientation-badge" aria-label="Top view orientation guide">
      <svg viewBox="0 0 124 124" role="img" aria-hidden="true">
        <circle className="orientation-ring" cx="62" cy="62" r="50" />
        <line className="orientation-axis" x1="62" y1="12" x2="62" y2="112" />
        <line className="orientation-axis" x1="12" y1="62" x2="112" y2="62" />
        <line className="orientation-link" x1="62" y1="62" x2={sourceX} y2={sourceY} />

        <g className="orientation-listener">
          <circle cx="62" cy="62" r="15" />
          <circle cx="48" cy="62" r="4" />
          <circle cx="76" cy="62" r="4" />
          <path d="M62 40 L56 52 L68 52 Z" />
        </g>

        <g className="orientation-source">
          <circle cx={sourceX} cy={sourceY} r="7" />
          <g
            style={{ opacity: 0.35 + directivity * 0.65 }}
            transform={`rotate(${sourceYaw} ${sourceX} ${sourceY})`}
          >
            <path
              className="orientation-source-beam"
              d={`
                M ${sourceX - 8} ${sourceY - 5}
                Q ${sourceX} ${sourceY - 22} ${sourceX + 8} ${sourceY - 5}
                L ${sourceX + 4} ${sourceY - 3}
                Q ${sourceX} ${sourceY - 12} ${sourceX - 4} ${sourceY - 3}
                Z
              `}
            />
            <path
              className="orientation-source-arrow"
              d={`
                M ${sourceX} ${sourceY - 18}
                L ${sourceX - 4} ${sourceY - 10}
                L ${sourceX + 4} ${sourceY - 10}
                Z
              `}
            />
          </g>
        </g>
      </svg>

      <div className="orientation-copy">
        <strong>Top view</strong>
        <span>Front is up. Positive azimuth turns to the listener&apos;s right.</span>
      </div>
    </div>
  );
}

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
  centerValue?: number;
  disabled?: boolean;
  hint?: string;
  onChange: (value: number) => void;
}) {
  const {
    label,
    value,
    min,
    max,
    step,
    unit,
    digits = 1,
    centerValue,
    disabled = false,
    hint,
    onChange,
  } = props;
  const centerPercent =
    centerValue == null ? null : ((centerValue - min) / (max - min)) * 100;

  return (
    <label className={`control-row${disabled ? " is-disabled" : ""}`}>
      <div className="control-meta">
        <span>{label}</span>
        <strong>
          {fmt(value, digits)}
          {unit}
        </strong>
      </div>
      <div className="slider-shell">
        {centerPercent != null ? (
          <span className="slider-center-mark" style={{ left: `${centerPercent}%` }} />
        ) : null}
        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={value}
          disabled={disabled}
          onChange={(event) => onChange(Number(event.target.value))}
          onDoubleClick={() => {
            if (centerValue != null) {
              onChange(centerValue);
            }
          }}
        />
      </div>
      <div className="control-footer">
        <span>{hint ?? (centerValue != null ? "Double-click to center" : "")}</span>
        {centerValue != null ? (
          <span>
            Center {fmt(centerValue, digits)}
            {unit}
          </span>
        ) : null}
      </div>
    </label>
  );
}

export default function App() {
  const [azimuth, setAzimuth] = useState(30);
  const [elevation, setElevation] = useState(0);
  const [distance, setDistance] = useState(1.5);
  const [sourceYaw, setSourceYaw] = useState(0);
  const [alwaysTowardsHead, setAlwaysTowardsHead] = useState(true);
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
      debugLog("effect_skipped_duplicate_mount");
      return;
    }
    didInit.current = true;
    debugLog("effect_mount", {
      guiVersion,
      loadedFrom,
      hasSendToPlugin: typeof window.sendToPlugin === "function",
    });

    window.onPluginMessage = (message: unknown) => {
      if (!isPluginMessage(message)) {
        debugLog("plugin_message_invalid", {
          payloadType: typeof message,
        });
        return;
      }

      debugLog("plugin_message_received", {
        messageType: message.type,
        initStage: message.type === "State" ? message.initStage ?? null : null,
        initMessage: message.type === "State" ? message.initMessage ?? null : null,
        hrtfReady: message.type === "State" ? Boolean(message.hrtfReady) : null,
      });

      if (message.type === "State") {
        debugLog("plugin_state_applied", {
          initStage: message.initStage ?? "idle",
          initMessage: message.initMessage ?? "Waiting for initialization",
          initProgress: message.initProgress ?? null,
          cachePath: message.cachePath ?? "",
          hrtfPath: message.hrtfPath ?? "",
          hrtfReady: Boolean(message.hrtfReady),
          alwaysTowardsHead: Boolean(message.alwaysTowardsHead ?? true),
        });
        setAzimuth(message.azimuth ?? 0);
        setElevation(message.elevation ?? 0);
        setDistance(message.distance ?? 1);
        setSourceYaw(message.sourceYaw ?? 0);
        setAlwaysTowardsHead(message.alwaysTowardsHead ?? true);
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
    debugLog("plugin_message_handler_registered");

    debugLog("init_dispatch_begin");
    sendToPluginSafe({ type: "Init" });
    debugLog("init_dispatch_end");

    return () => {
      debugLog("effect_unmount");
      window.onPluginMessage = undefined;
    };
  }, [guiVersion, loadedFrom]);

  const effectiveSourceYaw = alwaysTowardsHead ? getAutoSourceYaw(azimuth) : sourceYaw;
  const sourcePoint = useMemo(
    () => polarToCartesianTopView(azimuth, clamp(distance / 30, 0.1, 1) * 118),
    [azimuth, distance],
  );
  const directivityLabel = `${Math.round(directivity * 100)}%`;
  const visibleCoordinates = `az ${fmt(azimuth)} deg  |  el ${fmt(elevation)} deg  |  r ${fmt(distance, 2)} m`;
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
            <span>
              Top view: front is up, positive azimuth is to the listener&apos;s right. Audio stays
              silent until the HRTF cache is ready.
            </span>
          </div>

          <OrientationBadge
            azimuth={azimuth}
            sourceYaw={effectiveSourceYaw}
            directivity={directivity}
          />

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
                  transform: `translate(-12px, -52px) rotate(${effectiveSourceYaw}deg)`,
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
              <span>Heading</span>
              <strong>{alwaysTowardsHead ? "Auto" : "Manual"}</strong>
            </div>
            <div>
              <span>Output</span>
              <strong>{fmt(outputGain)} dB</strong>
            </div>
            <div>
              <span>Yaw</span>
              <strong>{fmt(effectiveSourceYaw)} deg</strong>
            </div>
          </div>
        </div>

        <div className="controls-card">
          <div className="section-label">Controls</div>

          <div className="toggle-card">
            <div>
              <strong>Always Towards Head</strong>
              <span>Automatically aim the source toward the listener.</span>
            </div>
            <label className="switch">
              <input
                type="checkbox"
                checked={alwaysTowardsHead}
                onChange={(event) => {
                  const value = event.target.checked;
                  setAlwaysTowardsHead(value);
                  sendToPluginSafe({ type: "SetAlwaysTowardsHead", value });
                }}
              />
              <span className="switch-track">
                <span className="switch-thumb"></span>
              </span>
            </label>
          </div>

          <div className="controls-note">
            Angle sliders center on double-click. Neutral center is 0 degrees.
          </div>

          <div className="control-group">
            <ControlRow
              label="Azimuth"
              value={azimuth}
              min={-180}
              max={180}
              step={0.1}
              unit=" deg"
              centerValue={0}
              onChange={(value) => {
                setAzimuth(value);
                sendToPluginSafe({ type: "SetAzimuth", value });
              }}
            />

            <ControlRow
              label="Elevation"
              value={elevation}
              min={-90}
              max={90}
              step={0.1}
              unit=" deg"
              centerValue={0}
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

            <ControlRow
              label="Source Yaw"
              value={sourceYaw}
              min={-180}
              max={180}
              step={0.1}
              unit=" deg"
              centerValue={0}
              disabled={alwaysTowardsHead}
              hint={
                alwaysTowardsHead
                  ? "Disabled while Always Towards Head is active"
                  : "Double-click to center"
              }
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
          <button
            className="validate-button"
            onClick={() => {
              debugLog("validate_cache_click");
              sendToPluginSafe({ type: "ValidateCache" });
            }}
          >
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
