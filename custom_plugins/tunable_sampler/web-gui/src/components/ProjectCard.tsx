type ProjectCardProps = {
  effectiveCacheDir: string | null;
  cacheDirOverride: string | null;
  projectSampleRate: number | null;
  cacheDirError: string | null;
  onPickCacheDir: () => void;
  onClearCacheDir: () => void;
};

export const ProjectCard = ({
  effectiveCacheDir,
  cacheDirOverride,
  projectSampleRate,
  cacheDirError,
  onPickCacheDir,
  onClearCacheDir,
}: ProjectCardProps) => {
  const isCustom = cacheDirOverride !== null;

  return (
    <div className="project-card">
      <div className="section-label">Cache Directory</div>
      <div className="project-path">
        {effectiveCacheDir ?? "Resolving..."}
        {isCustom ? <span className="project-meta"> (custom)</span> : null}
      </div>
      <div className="project-meta">
        Host sample rate:{" "}
        {projectSampleRate === null ? "--" : `${projectSampleRate} Hz`}
      </div>
      {cacheDirError ? (
        <div className="project-error">{cacheDirError}</div>
      ) : null}
      <div className="project-actions">
        <button className="pick-button" type="button" onClick={onPickCacheDir}>
          {isCustom ? "Change Custom Dir" : "Set Custom Dir"}
        </button>
        {isCustom ? (
          <button
            className="pick-button"
            type="button"
            onClick={onClearCacheDir}
          >
            Use Default
          </button>
        ) : null}
      </div>
    </div>
  );
};
