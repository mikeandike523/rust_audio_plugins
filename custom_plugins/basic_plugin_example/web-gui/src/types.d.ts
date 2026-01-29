export {};

declare global {
  interface Window {
    sendToPlugin?: (payload: unknown) => void;
    onPluginMessage?: (message: unknown) => void;
  }
}
