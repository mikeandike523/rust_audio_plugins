import type { ReactNode } from "react";

type ProjectCardProps = {
  projectFolder: string | null;
  projectName: string | null;
  cacheFolder: string | null;
  projectSampleRate: number | null;
  folderError: string | null;
  onPickFolder: () => void;
};

export const ProjectCard = ({
  projectFolder,
  projectName,
  cacheFolder,
  projectSampleRate,
  folderError,
  onPickFolder,
}: ProjectCardProps) => {
  const projectMeta: ReactNode[] = [];

  if (projectName) {
    projectMeta.push(
      <div className="project-meta" key="project-name">
        Project: {projectName}
      </div>,
    );
  }

  if (cacheFolder) {
    projectMeta.push(
      <div className="project-meta" key="cache-folder">
        Cache: {cacheFolder}
      </div>,
    );
  }

  projectMeta.push(
    <div className="project-meta" key="host-rate">
      Host sample rate:{" "}
      {projectSampleRate === null ? "--" : `${projectSampleRate} Hz`}
    </div>,
  );

  return (
    <div className="project-card">
      <div className="section-label">Project Folder</div>
      <div className="project-path">
        {projectFolder ?? "No folder selected"}
      </div>
      {projectMeta}
      {folderError ? <div className="project-error">{folderError}</div> : null}
      <div className="project-actions">
        <button className="pick-button" type="button" onClick={onPickFolder}>
          {projectFolder ? "Change Folder" : "Pick Folder"}
        </button>
      </div>
    </div>
  );
};
