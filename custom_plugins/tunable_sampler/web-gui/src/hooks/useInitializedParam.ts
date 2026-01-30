import { useCallback, useEffect, useMemo, useState } from "react";

type UseInitializedParamOptions<T> = {
  name: string;
  initialValue?: T | null;
  requestPayload: unknown;
  sendPayload?: (value: T) => unknown;
  pollMs?: number | null;
};

export const sendToPluginSafe = (payload: unknown) => {
  if (typeof window.sendToPlugin === "function") {
    window.sendToPlugin(payload);
    return true;
  }
  console.info("sendToPlugin missing", payload);
  return false;
};

export const useInitializedParam = <T,>({
  name,
  initialValue = null,
  requestPayload,
  sendPayload,
  pollMs = 100,
}: UseInitializedParamOptions<T>) => {
  const [value, setValueState] = useState<T | null>(initialValue);
  const [initialized, setInitialized] = useState(initialValue !== null);

  const setFromPlugin = useCallback((next: T | null) => {
    setValueState(next);
    setInitialized(true);
  }, []);

  const setValue = useCallback(
    (next: T) => {
      setValueState(next);
      if (sendPayload) {
        const sent = sendToPluginSafe(sendPayload(next));
        if (sent) {
          setInitialized(true);
        }
      } else {
        setInitialized(true);
        console.info("No send handler for", name);
      }
    },
    [name, sendPayload],
  );

  useEffect(() => {
    if (initialized || pollMs === null || pollMs <= 0) {
      return;
    }

    const intervalId = window.setInterval(() => {
      sendToPluginSafe(requestPayload);
    }, pollMs);

    return () => {
      window.clearInterval(intervalId);
    };
  }, [initialized, pollMs, requestPayload]);

  const ready = useMemo(() => initialized, [initialized]);

  return { ready, setFromPlugin, setValue, value };
};
