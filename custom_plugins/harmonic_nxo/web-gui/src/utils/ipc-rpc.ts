export type RpcSend = (payload: Record<string, unknown>) => void;

export type RpcAction =
  | { type: 'SetRequestedFunction'; id: number; name: string; args: unknown[] }
  | { type: 'SetFunctionResult'; id: number; result: unknown }
  | { type: 'SendFunctionResult'; id: number };

export class IpcRpc {
  private funcs: Record<string, (...args: any[]) => any | Promise<any>> = {};
  private pendingSend = new Map<number, unknown>();
  private incoming = new Map<number, unknown>();
  private nextId = 0;

  constructor(private send: RpcSend) {}

  registerFunction(name: string, fn: (...args: any[]) => any | Promise<any>) {
    this.funcs[name] = fn;
  }

  callRemoteFunction(name: string, args: unknown[]): number {
    const id = this.nextId++;
    this.send({ type: 'SetRequestedFunction', id, name, args });
    this.send({ type: 'SendFunctionResult', id });
    return id;
  }

  tryTakeResult(id: number): unknown | undefined {
    const res = this.incoming.get(id);
    if (res !== undefined) {
      this.incoming.delete(id);
    }
    return res;
  }

  handleMessage(msg: RpcAction) {
    switch (msg.type) {
      case 'SetRequestedFunction': {
        const fn = this.funcs[msg.name];
        if (fn) {
          Promise.resolve(fn(...msg.args)).then((r) => {
            this.pendingSend.set(msg.id, r);
          });
        } else {
          this.pendingSend.set(msg.id, null);
        }
        break;
      }
      case 'SendFunctionResult': {
        if (this.pendingSend.has(msg.id)) {
          this.send({
            type: 'SetFunctionResult',
            id: msg.id,
            result: this.pendingSend.get(msg.id),
          });
          this.pendingSend.delete(msg.id);
        }
        break;
      }
      case 'SetFunctionResult': {
        this.incoming.set(msg.id, msg.result);
        break;
      }
    }
  }
}
