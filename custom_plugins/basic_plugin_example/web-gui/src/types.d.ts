export {};

declare global {
  interface Window {
    sendToPlugin?: (payload: unknown) => void;
    onPluginMessage?: (message: unknown) => void;
  }

  interface ImportMetaEnv {
    readonly VITE_GUI_VERSION?: string;
  }

  interface ImportMeta {
    readonly env: ImportMetaEnv;
  }
}
