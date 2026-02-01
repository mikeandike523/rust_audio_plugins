import type { ResampleModalState } from "../types/appTypes";

type ResampleModalProps = {
  resampleModal: ResampleModalState | null;
  resampleFading: boolean;
};

export const ResampleModal = ({
  resampleModal,
  resampleFading,
}: ResampleModalProps) => {
  if (!resampleModal) {
    return null;
  }

  return (
    <div className={`progress-backdrop${resampleFading ? " is-fading" : ""}`}>
      <div className="progress-modal" role="status" aria-live="polite">
        <div className="progress-title">{resampleModal.label}</div>
        <div className="progress-bar">
          <div
            className="progress-fill"
            style={{ width: `${Math.round(resampleModal.progress * 100)}%` }}
          />
        </div>
        <div className="progress-copy">
          {resampleModal.message ?? `${Math.round(resampleModal.progress * 100)}%`}
        </div>
      </div>
    </div>
  );
};
