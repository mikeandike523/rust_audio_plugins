import { useEffect, useMemo, useRef, useState } from "react";
import { debugLog } from "./debugLog";

type MeterValues = {
  l: number;
  r: number;
};

type SofaOption = {
  key: string;
  name: string;
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
      radialMultiply?: number;
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
      // Pinna
      pinnaEnabled?: boolean;
      pinnaFreq?: number;
      pinnaGainDb?: number;
      pinnaQ?: number;
      // HRTF engine
      hrtfInterpolate?: boolean;
      itdEnabled?: boolean;
      useSymmetricHrtf?: boolean;
      analyticModelType?: string;
      headRadiusCm?: number;
      // Distance model
      distanceExponent?: number;
      distanceMinM?: number;
      // Directivity model
      directivityFloor?: number;
      directivityRange?: number;
      directivityExpScale?: number;
      // Reverb
      reverbEnabled?: boolean;
      reverbWet?: number;
      reverbRoomSize?: number;
      reverbPreDelayMs?: number;
      reverbDamping?: number;
      // SOFA selection
      sofaKey?: string;
      sofaOptions?: SofaOption[];
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

function ToggleRow(props: {
  label: string;
  description?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  const { label, description, checked, onChange } = props;
  return (
    <div className="toggle-card">
      <div>
        <strong>{label}</strong>
        {description ? <span>{description}</span> : null}
      </div>
      <label className="switch">
        <input
          type="checkbox"
          checked={checked}
          onChange={(event) => onChange(event.target.checked)}
        />
        <span className="switch-track">
          <span className="switch-thumb"></span>
        </span>
      </label>
    </div>
  );
}

export default function App() {
  // --- Spatial ---
  const [azimuth, setAzimuth] = useState(30);
  const [elevation, setElevation] = useState(0);
  const [distance, setDistance] = useState(1.5);
  const [sourceYaw, setSourceYaw] = useState(0);
  const [alwaysTowardsHead, setAlwaysTowardsHead] = useState(true);
  const [directivity, setDirectivity] = useState(0.65);
  const [outputGain, setOutputGain] = useState(-3);
  const [radialMultiply, setRadialMultiply] = useState(1.0);

  // --- Pinna pre-filter ---
  const [pinnaEnabled, setPinnaEnabled] = useState(true);
  const [pinnaFreq, setPinnaFreq] = useState(8000);
  const [pinnaGainDb, setPinnaGainDb] = useState(4.0);
  const [pinnaQ, setPinnaQ] = useState(0.88);

  // --- HRTF engine ---
  const [hrtfInterpolate, setHrtfInterpolate] = useState(true);
  const [itdEnabled, setItdEnabled] = useState(true);
  const [useSymmetricHrtf, setUseSymmetricHrtf] = useState(false);
  const [analyticModelType, setAnalyticModelType] = useState("woodworth");
  const [headRadiusCm, setHeadRadiusCm] = useState(8.75);

  // --- Distance model ---
  const [distanceExponent, setDistanceExponent] = useState(1.0);
  const [distanceMinM, setDistanceMinM] = useState(1.0);

  // --- Directivity model ---
  const [directivityFloor, setDirectivityFloor] = useState(0.15);
  const [directivityRange, setDirectivityRange] = useState(0.85);
  const [directivityExpScale, setDirectivityExpScale] = useState(3.0);

  // --- Reverb ---
  const [reverbEnabled, setReverbEnabled] = useState(false);
  const [reverbWet, setReverbWet] = useState(0.15);
  const [reverbRoomSize, setReverbRoomSize] = useState(0.5);
  const [reverbPreDelayMs, setReverbPreDelayMs] = useState(20.0);
  const [reverbDamping, setReverbDamping] = useState(0.5);

  // --- SOFA selection ---
  const [sofaKey, setSofaKey] = useState("HRIR_FULL2DEG");
  const [sofaOptions, setSofaOptions] = useState<SofaOption[]>([]);

  // --- Init / status ---
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
        // Spatial
        setAzimuth(message.azimuth ?? 0);
        setElevation(message.elevation ?? 0);
        setDistance(message.distance ?? 1);
        setSourceYaw(message.sourceYaw ?? 0);
        setAlwaysTowardsHead(message.alwaysTowardsHead ?? true);
        setDirectivity(message.directivity ?? 0);
        setOutputGain(message.outputGain ?? 0);
        setRadialMultiply(message.radialMultiply ?? 1.0);
        // Pinna
        setPinnaEnabled(message.pinnaEnabled ?? true);
        setPinnaFreq(message.pinnaFreq ?? 8000);
        setPinnaGainDb(message.pinnaGainDb ?? 4.0);
        setPinnaQ(message.pinnaQ ?? 0.88);
        // HRTF engine
        setHrtfInterpolate(message.hrtfInterpolate ?? true);
        setItdEnabled(message.itdEnabled ?? true);
        setUseSymmetricHrtf(message.useSymmetricHrtf ?? false);
        setAnalyticModelType(message.analyticModelType ?? "woodworth");
        setHeadRadiusCm(message.headRadiusCm ?? 8.75);
        // Distance model
        setDistanceExponent(message.distanceExponent ?? 1.0);
        setDistanceMinM(message.distanceMinM ?? 1.0);
        // Directivity model
        setDirectivityFloor(message.directivityFloor ?? 0.15);
        setDirectivityRange(message.directivityRange ?? 0.85);
        setDirectivityExpScale(message.directivityExpScale ?? 3.0);
        // Reverb
        setReverbEnabled(message.reverbEnabled ?? false);
        setReverbWet(message.reverbWet ?? 0.15);
        setReverbRoomSize(message.reverbRoomSize ?? 0.5);
        setReverbPreDelayMs(message.reverbPreDelayMs ?? 20.0);
        setReverbDamping(message.reverbDamping ?? 0.5);
        // SOFA selection
        if (message.sofaKey != null) setSofaKey(message.sofaKey);
        if (message.sofaOptions != null) setSofaOptions(message.sofaOptions);
        // Plugin info / status
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

          <ToggleRow
            label="Always Towards Head"
            description="Automatically aim the source toward the listener."
            checked={alwaysTowardsHead}
            onChange={(value) => {
              setAlwaysTowardsHead(value);
              sendToPluginSafe({ type: "SetAlwaysTowardsHead", value });
            }}
          />

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

            <ControlRow
              label="Radial Multiply"
              value={radialMultiply}
              min={-1.0}
              max={1.0}
              step={0.01}
              unit="×"
              digits={2}
              centerValue={1.0}
              hint="Scales horizontal position. Negative flips front↔back. At azimuth 0°: 1.0=front, −1.0=rear."
              onChange={(value) => {
                setRadialMultiply(value);
                sendToPluginSafe({ type: "SetRadialMultiply", value });
              }}
            />
          </div>
        </div>
      </section>

      {/* ------------------------------------------------------------------ */}
      {/* Signal Processing Controls                                          */}
      {/* ------------------------------------------------------------------ */}
      <section className="panel signal-section">
        <div className="section-label" style={{ padding: "18px 18px 0" }}>
          Signal Processing
        </div>

        <div className="signal-grid">
          {/* ---- HRTF Source (full width) ---- */}
          <div className="signal-card signal-card--wide">
            <div className="signal-card-title">HRTF Source</div>

            <div className="signal-card-body">
              <div className="signal-col">
                <div className="signal-sublabel">SOFA File</div>
                <div className="radio-group">
                  {sofaOptions.length === 0 ? (
                    <div className="radio-row is-placeholder">
                      <span>{sofaKey}</span>
                      <span className="radio-hint">Loading options…</span>
                    </div>
                  ) : (
                    sofaOptions.map((opt) => (
                      <button
                        key={opt.key}
                        className={`radio-row${sofaKey === opt.key ? " is-selected" : ""}`}
                        onClick={() => {
                          setSofaKey(opt.key);
                          sendToPluginSafe({ type: "SetSofaSelection", key: opt.key });
                        }}
                      >
                        <span className="radio-dot" aria-hidden="true"></span>
                        <span className="radio-label">{opt.name}</span>
                        <span className="radio-key">{opt.key}</span>
                      </button>
                    ))
                  )}
                </div>
              </div>

              <div className="signal-col">
                <div className="signal-sublabel">Lookup Mode</div>
                <ToggleRow
                  label="HRTF Interpolation"
                  description="Tri-linear interpolation between measured directions. Disable for nearest-neighbor."
                  checked={hrtfInterpolate}
                  onChange={(value) => {
                    setHrtfInterpolate(value);
                    sendToPluginSafe({ type: "SetHrtfInterpolate", value });
                  }}
                />
                <ToggleRow
                  label="ITD Delays"
                  description="Apply interaural time delay from SOFA data. Disabling removes per-ear sample offset."
                  checked={itdEnabled}
                  onChange={(value) => {
                    setItdEnabled(value);
                    sendToPluginSafe({ type: "SetItdEnabled", value });
                  }}
                />
              </div>

              <div className="signal-col">
                <div className="signal-sublabel">Symmetric Model</div>
                <ToggleRow
                  label="Symmetric HRTF"
                  description="Replace measured azimuthal cues with an analytic head model. Elevation and front-back coloration remain from the SOFA data."
                  checked={useSymmetricHrtf}
                  onChange={(value) => {
                    setUseSymmetricHrtf(value);
                    sendToPluginSafe({ type: "SetUseSymmetricHrtf", value });
                  }}
                />
                <div className={`control-group${useSymmetricHrtf ? "" : " is-section-disabled"}`}>
                  <div className="signal-sublabel" style={{ marginTop: 8 }}>ITD / ILD Model</div>
                  <div className="radio-group">
                    {(["woodworth", "duda"] as const).map((model) => (
                      <button
                        key={model}
                        className={`radio-row${analyticModelType === model ? " is-selected" : ""}${!useSymmetricHrtf ? " is-disabled" : ""}`}
                        disabled={!useSymmetricHrtf}
                        onClick={() => {
                          setAnalyticModelType(model);
                          sendToPluginSafe({ type: "SetAnalyticModelType", value: model });
                        }}
                      >
                        <span className="radio-dot" aria-hidden="true"></span>
                        <span className="radio-label">
                          {model === "woodworth" ? "Woodworth (1938)" : "Duda & Martens (1998)"}
                        </span>
                      </button>
                    ))}
                  </div>
                  <ControlRow
                    label="Head Radius"
                    value={headRadiusCm}
                    min={7}
                    max={11}
                    step={0.05}
                    unit=" cm"
                    digits={2}
                    centerValue={8.75}
                    disabled={!useSymmetricHrtf}
                    hint="Average adult head radius ≈ 8.75 cm"
                    onChange={(value) => {
                      setHeadRadiusCm(value);
                      sendToPluginSafe({ type: "SetHeadRadiusCm", value });
                    }}
                  />
                </div>
              </div>
            </div>
          </div>

          {/* ---- Pinna Pre-Filter ---- */}
          <div className="signal-card">
            <div className="signal-card-title">Pinna Pre-Filter</div>
            <ToggleRow
              label="Enable"
              description="Peaking EQ boost in the pinna-relevant frequency band."
              checked={pinnaEnabled}
              onChange={(value) => {
                setPinnaEnabled(value);
                sendToPluginSafe({ type: "SetPinnaEnabled", value });
              }}
            />
            <div className={`control-group${pinnaEnabled ? "" : " is-section-disabled"}`}>
              <ControlRow
                label="Center Frequency"
                value={pinnaFreq}
                min={2000}
                max={16000}
                step={10}
                unit=" Hz"
                digits={0}
                disabled={!pinnaEnabled}
                onChange={(value) => {
                  setPinnaFreq(value);
                  sendToPluginSafe({ type: "SetPinnaFreq", value });
                }}
              />
              <ControlRow
                label="Gain"
                value={pinnaGainDb}
                min={-12}
                max={12}
                step={0.1}
                unit=" dB"
                centerValue={0}
                disabled={!pinnaEnabled}
                onChange={(value) => {
                  setPinnaGainDb(value);
                  sendToPluginSafe({ type: "SetPinnaGainDb", value });
                }}
              />
              <ControlRow
                label="Q"
                value={pinnaQ}
                min={0.1}
                max={8}
                step={0.01}
                unit=""
                digits={2}
                disabled={!pinnaEnabled}
                onChange={(value) => {
                  setPinnaQ(value);
                  sendToPluginSafe({ type: "SetPinnaQ", value });
                }}
              />
            </div>
          </div>

          {/* ---- Distance Model ---- */}
          <div className="signal-card">
            <div className="signal-card-title">Distance Model</div>
            <p className="signal-card-note">
              Gain = distance<sup>−exp</sup>. Exponent 1 = inverse distance law.
              Distance Min clamps the minimum rendering distance to avoid clipping.
            </p>
            <div className="control-group">
              <ControlRow
                label="Distance Exponent"
                value={distanceExponent}
                min={0}
                max={2}
                step={0.01}
                unit=""
                digits={2}
                centerValue={1.0}
                hint="0 = flat; 1 = inverse distance; 2 = inverse square"
                onChange={(value) => {
                  setDistanceExponent(value);
                  sendToPluginSafe({ type: "SetDistanceExponent", value });
                }}
              />
              <ControlRow
                label="Distance Minimum"
                value={distanceMinM}
                min={0.1}
                max={5}
                step={0.01}
                unit=" m"
                digits={2}
                hint="Clamps effective distance to prevent excessive gain at close range"
                onChange={(value) => {
                  setDistanceMinM(value);
                  sendToPluginSafe({ type: "SetDistanceMinM", value });
                }}
              />
            </div>
          </div>

          {/* ---- Directivity Model (full width) ---- */}
          <div className="signal-card signal-card--wide">
            <div className="signal-card-title">Directivity Model</div>
            <p className="signal-card-note">
              Cardioid-based directivity: gain = lerp(1, (floor + range × cardioid)<sup>1 + amount × expScale</sup>, amount).
              Floor prevents complete silence at rear. Range controls rolloff depth. Exp Scale sharpens the beam.
            </p>
            <div className="control-group">
              <ControlRow
                label="Floor"
                value={directivityFloor}
                min={0}
                max={0.5}
                step={0.01}
                unit=""
                digits={2}
                hint="Minimum gain at the rear (0 = silent at 180°)"
                onChange={(value) => {
                  setDirectivityFloor(value);
                  sendToPluginSafe({ type: "SetDirectivityFloor", value });
                }}
              />
              <ControlRow
                label="Range"
                value={directivityRange}
                min={0}
                max={1}
                step={0.01}
                unit=""
                digits={2}
                hint="Cardioid weight — how much front-vs-rear matters"
                onChange={(value) => {
                  setDirectivityRange(value);
                  sendToPluginSafe({ type: "SetDirectivityRange", value });
                }}
              />
              <ControlRow
                label="Exp Scale"
                value={directivityExpScale}
                min={0.5}
                max={10}
                step={0.05}
                unit=""
                digits={2}
                hint="Sharpens the directivity curve — higher = tighter beam"
                onChange={(value) => {
                  setDirectivityExpScale(value);
                  sendToPluginSafe({ type: "SetDirectivityExpScale", value });
                }}
              />
            </div>
          </div>

          {/* ---- Room / Reverb (full width) ---- */}
          <div className="signal-card signal-card--wide">
            <div className="signal-card-title">Room / Reverb</div>
            <ToggleRow
              label="Enable Reverb"
              description="Freeverb (Schroeder-Moorer) diffuse field reverb added on top of the HRTF output."
              checked={reverbEnabled}
              onChange={(value) => {
                setReverbEnabled(value);
                sendToPluginSafe({ type: "SetReverbEnabled", value });
              }}
            />
            <div className={`control-group${reverbEnabled ? "" : " is-section-disabled"}`}>
              <ControlRow
                label="Wet Mix"
                value={reverbWet}
                min={0}
                max={1}
                step={0.01}
                unit=""
                digits={2}
                disabled={!reverbEnabled}
                hint="Amount of reverb signal mixed into the output"
                onChange={(value) => {
                  setReverbWet(value);
                  sendToPluginSafe({ type: "SetReverbWet", value });
                }}
              />
              <ControlRow
                label="Room Size"
                value={reverbRoomSize}
                min={0}
                max={1}
                step={0.01}
                unit=""
                digits={2}
                centerValue={0.5}
                disabled={!reverbEnabled}
                hint="Maps to comb filter feedback — higher = longer decay"
                onChange={(value) => {
                  setReverbRoomSize(value);
                  sendToPluginSafe({ type: "SetReverbRoomSize", value });
                }}
              />
              <ControlRow
                label="Pre-Delay"
                value={reverbPreDelayMs}
                min={0}
                max={100}
                step={0.5}
                unit=" ms"
                digits={1}
                disabled={!reverbEnabled}
                hint="Delay before reverb onset — simulates room size / distance to first reflection"
                onChange={(value) => {
                  setReverbPreDelayMs(value);
                  sendToPluginSafe({ type: "SetReverbPreDelayMs", value });
                }}
              />
              <ControlRow
                label="Damping"
                value={reverbDamping}
                min={0}
                max={1}
                step={0.01}
                unit=""
                digits={2}
                disabled={!reverbEnabled}
                hint="High-frequency damping in the comb filters — higher = darker reverb tail"
                onChange={(value) => {
                  setReverbDamping(value);
                  sendToPluginSafe({ type: "SetReverbDamping", value });
                }}
              />
            </div>
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
