export interface NIHPlugWebviewWindow {
    
    sendToPlugin:<T extends Record<string, unknown>>(payload: T)=> void
    onPluginMessage?: <T extends Record<string, unknown>>(payload: T)=> void

}