import { LuaFactory } from 'wasmoon';

const factory = new LuaFactory();

self.onmessage = async (ev: MessageEvent<{ id: number; code: string }>) => {
  const { id, code } = ev.data;
  try {
    const lua = await factory.createEngine();
    const result = await lua.doString(code);
    self.postMessage({ id, result });
  } catch (err) {
    const message = err instanceof Error ? err.stack || err.message : String(err);
    self.postMessage({ id, error: message });
  }
};
