import { useEffect, useMemo, useRef, useState } from "react";
import { sendToPluginSafe, useInitializedParam } from "./hooks/useInitializedParam";

type PluginMessage =
  | {
      type: "State";
      pluginVersion?: string | null;
      projectFolder?: string | null;
      cachePath?: string | null;
      projectName?: string | null;
      gain?: number | null;
    }
  | {
      type: "ProjectFolderError";
      message: string;
    };

const clamp = (value: number, min: number, max: number) =>
  Math.min(max, Math.max(min, value));

export default function App() {
  const [status, setStatus] = useState("Waiting for plugin...");
  const [cacheFolder, setCacheFolder] = useState<string | null>(null);
  const [folderError, setFolderError] = useState<string | null>(null);
  const [loadedFrom] = useState(() => window.location.href);
  const didInit = useRef(false);
  const guiVersion = import.meta.env.VITE_GUI_VERSION ?? "dev";
  const projectFolderInputRef = useRef<HTMLInputElement | null>(null);
  const requestStatePayload = useMemo(() => ({ type: "RequestState" }), []);

  const pluginVersionParam = useInitializedParam<string>({
    name: "pluginVersion",
    requestPayload: requestStatePayload,
  });

  const projectFolderParam = useInitializedParam<string>({
    name: "projectFolder",
    requestPayload: requestStatePayload,
    sendPayload: (value) => ({ type: "SetProjectFolder", path: value }),
  });

  const projectNameParam = useInitializedParam<string>({
    name: "projectName",
    requestPayload: requestStatePayload,
  });

  const gainParam = useInitializedParam<number>({
    name: "gain",
    requestPayload: requestStatePayload,
    sendPayload: (value) => ({ type: "SetGain", value }),
  });

  const [projectFolderDraft, setProjectFolderDraft] = useState("");

  useEffect(() => {
    if (didInit.current) {
      return;
    }
    didInit.current = true;

    window.onPluginMessage = (message: PluginMessage) => {
      if (message.type === "State") {
        let nextStatus = "Connected";
        if (typeof message.pluginVersion === "string") {
          pluginVersionParam.setFromPlugin(message.pluginVersion);
        }
        if (message.projectFolder === null) {
          projectFolderParam.setFromPlugin(null);
        } else if (typeof message.projectFolder === "string") {
          projectFolderParam.setFromPlugin(message.projectFolder);
          setFolderError(null);
          nextStatus = "Project folder set";
        }
        if (message.cachePath === null) {
          setCacheFolder(null);
        } else if (typeof message.cachePath === "string") {
          setCacheFolder(message.cachePath);
        }
        if (message.projectName === null) {
          projectNameParam.setFromPlugin(null);
        } else if (typeof message.projectName === "string") {
          projectNameParam.setFromPlugin(message.projectName);
        }
        if (message.gain === null) {
          gainParam.setFromPlugin(null);
        } else if (typeof message.gain === "number") {
          gainParam.setFromPlugin(clamp(message.gain, -24, 24));
        }
        setStatus(nextStatus);
      }

      if (message.type === "ProjectFolderError") {
        setFolderError(message.message);
        setStatus("Project folder error");
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
    if (document.activeElement === projectFolderInputRef.current) {
      return;
    }
    if (projectFolderParam.value === null) {
      setProjectFolderDraft("");
      return;
    }
    setProjectFolderDraft(projectFolderParam.value);
  }, [projectFolderParam.value]);

  const handleProjectFolderCommit = () => {
    const trimmed = projectFolderDraft.trim();
    if (!trimmed) {
      setFolderError("Please enter a project folder path.");
      return;
    }
    setFolderError(null);
    setStatus("Setting project folder...");
    projectFolderParam.setValue(trimmed);
  };

  const handleGainChange = (value: number) => {
    const clamped = clamp(value, -24, 24);
    gainParam.setValue(clamped);
  };

  return (
    <div className="panel">
      <header>
        <div>
          <h1>Tunable Sampler</h1>
          <div className="subtitle">Instrument Setup</div>
        </div>
        <div className="source">
          <div className="source-label">Loaded From</div>
          <div className="source-value" title={loadedFrom}>
            {loadedFrom}
          </div>
        </div>
      </header>

      <section className="setup">
        <div className="setup-header">
          <div>
            <div className="section-label">Project Folder</div>
            <div className="section-subtitle">
              Paste the DAW project folder path to initialize the sampler.
            </div>
          </div>
        </div>
        <div className="path-input">
          <input
            ref={projectFolderInputRef}
            type="text"
            placeholder="C:\\Projects\\MySamplerProject"
            value={projectFolderDraft}
            onChange={(event) => setProjectFolderDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                handleProjectFolderCommit();
              }
            }}
          />
          <button className="apply-button" onClick={handleProjectFolderCommit}>
            Set Folder
          </button>
        </div>
        <div className="drop-zone">
          <div className="drop-title">Current project</div>
          <div className="drop-path">
            {projectFolderParam.value ?? "No folder selected"}
          </div>
          {projectNameParam.value ? (
            <div className="drop-cache">Project: {projectNameParam.value}</div>
          ) : null}
          {cacheFolder ? (
            <div className="drop-cache">Cache: {cacheFolder}</div>
          ) : null}
          {folderError ? <div className="drop-error">{folderError}</div> : null}
        </div>
      </section>

      <section className="controls">
        <div className="control">
          <label htmlFor="gain">Gain</label>
          <input
            id="gain"
            type="range"
            min="-24"
            max="24"
            step="0.1"
            value={gainParam.value ?? 0}
            onChange={(event) => handleGainChange(Number(event.target.value))}
            disabled={gainParam.value === null}
          />
          <div className="value">
            {gainParam.value === null ? "--" : `${gainParam.value.toFixed(1)} dB`}
          </div>
        </div>
      </section>

      <div className="version-meta">
        <div>plugin-version: {pluginVersionParam.value ?? "unknown"}</div>
        <div>gui-version: {guiVersion}</div>
      </div>

      <div className="footer">{status}</div>
    </div>
  );
}
