import { useCallback, useState } from "react";

export type LoadingTask = { id: string; message: string };

/**
 * Manages a set of concurrent loading tasks.
 *
 * Each task has a stable string id (e.g. "decode", "resample", "pitch").
 * The overlay shows whenever tasks.length > 0.
 * addTask is idempotent — calling it with an existing id updates the message.
 */
export function useLoadingTasks() {
  const [tasks, setTasks] = useState<LoadingTask[]>([]);

  const addTask = useCallback((id: string, message: string) => {
    setTasks((prev) => {
      if (prev.some((t) => t.id === id)) {
        return prev.map((t) => (t.id === id ? { id, message } : t));
      }
      return [...prev, { id, message }];
    });
  }, []);

  const updateTask = useCallback((id: string, message: string) => {
    setTasks((prev) => prev.map((t) => (t.id === id ? { id, message } : t)));
  }, []);

  const removeTask = useCallback((id: string) => {
    setTasks((prev) => prev.filter((t) => t.id !== id));
  }, []);

  return {
    tasks,
    isLoading: tasks.length > 0,
    addTask,
    updateTask,
    removeTask,
  };
}
