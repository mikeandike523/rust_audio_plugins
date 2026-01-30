import { useCallback, useEffect, useMemo, useState } from "react";

type UseInitializedParamOptions<T> = {
  name: string;
  initialValue?: T | null;
  requestPayload: unknown;
  sendPayload?: (value: T) => unknown;
  pollMs?: number;
};

export const sendToPluginSafe = (payload: unknown) => {
  if (typeof window.sendToPlugin === "function") {
    window.sendToPlugin(payload);
  } else {
    console.info("sendToPlugin missing", payload);
  }
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
      setInitialized(true);
      if (sendPayload) {
        sendToPluginSafe(sendPayload(next));
      } else {
        console.info("No send handler for", name);
      }
    },
    [name, sendPayload],
  );

  useEffect(() => {
    if (initialized) {
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
