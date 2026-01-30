import { useEffect, useRef, useState } from "react";

type PluginMessage =
  | {
      type: "PluginInfo";
      pluginVersion?: string;
    }
  | {
      type: "ProjectFolderSelected";
      path: string;
      cachePath: string;
    }
  | {
      type: "ProjectFolderError";
      message: string;
    };

const sendToPluginSafe = (payload: unknown) => {
  if (typeof window.sendToPlugin === "function") {
    window.sendToPlugin(payload);
  } else {
    console.info("sendToPlugin missing", payload);
  }
};

export default function App() {
  const [status, setStatus] = useState("Waiting for plugin...");
  const [pluginVersion, setPluginVersion] = useState<string | null>(null);
  const [projectFolder, setProjectFolder] = useState<string | null>(null);
  const [cacheFolder, setCacheFolder] = useState<string | null>(null);
  const [folderError, setFolderError] = useState<string | null>(null);
  const [loadedFrom] = useState(() => window.location.href);
  const didInit = useRef(false);
  const guiVersion = import.meta.env.VITE_GUI_VERSION ?? "dev";

  useEffect(() => {
    if (didInit.current) {
      return;
    }
    didInit.current = true;

    window.onPluginMessage = (message: PluginMessage) => {
      if (message.type === "PluginInfo") {
        if (typeof message.pluginVersion === "string") {
          setPluginVersion(message.pluginVersion);
        }
        setStatus("Connected");
      }

      if (message.type === "ProjectFolderSelected") {
        setProjectFolder(message.path);
        setCacheFolder(message.cachePath);
        setFolderError(null);
        setStatus("Project folder set");
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
    if (pluginVersion) {
      return;
    }

    const intervalId = window.setInterval(() => {
      sendToPluginSafe({ type: "RequestPluginInfo" });
    }, 100);

    return () => {
      window.clearInterval(intervalId);
    };
  }, [pluginVersion]);

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
              Drag and drop the DAW project folder onto this window.
            </div>
          </div>
        </div>
        <div className="drop-zone">
          <div className="drop-title">Drop a folder to set the project</div>
          <div className="drop-path">
            {projectFolder ?? "No folder selected"}
          </div>
          {cacheFolder ? (
            <div className="drop-cache">Cache: {cacheFolder}</div>
          ) : null}
          {folderError ? <div className="drop-error">{folderError}</div> : null}
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
