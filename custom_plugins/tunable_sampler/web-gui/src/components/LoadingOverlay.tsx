import type { LoadingTask } from "../hooks/useLoadingTasks";

type LoadingOverlayProps = {
  tasks: LoadingTask[];
};

export const LoadingOverlay = ({ tasks }: LoadingOverlayProps) => {
  if (tasks.length === 0) return null;

  return (
    <div className="loading-backdrop">
      <div className="loading-modal">
        <div className="loading-spinner" />
        <div className="loading-tasks">
          {tasks.map((t) => (
            <div key={t.id} className="loading-task-message">
              {t.message}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};
