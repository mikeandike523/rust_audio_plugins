type ProjectFolderModalProps = {
  show: boolean;
  folderError: string | null;
  onPickFolder: () => void;
};

export const ProjectFolderModal = ({
  show,
  folderError,
  onPickFolder,
}: ProjectFolderModalProps) => {
  if (!show) {
    return null;
  }

  return (
    <div className="modal-backdrop">
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="project-folder-required-title"
      >
        <div className="modal-title" id="project-folder-required-title">
          Select a project folder to continue
        </div>
        <div className="modal-copy">
          This sampler needs your DAW project folder before the rest of the
          controls unlock.
        </div>
        <div className="path-input modal-input">
          <button className="pick-button" type="button" onClick={onPickFolder}>
            Pick Folder
          </button>
        </div>
        {folderError ? <div className="modal-error">{folderError}</div> : null}
      </div>
    </div>
  );
};
