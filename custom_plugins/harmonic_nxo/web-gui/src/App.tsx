import { useEffect, useMemo, useRef, useState } from "react";
import { Button, Div, H1, P, Span } from "style-props-html";
import Slider from "react-slider";
import { type NIHPlugWebviewWindow } from "./nih-plug-webview-window";
import "../styles/sliders.css";
import lodash from "lodash";
import Editor, { type OnMount } from "@monaco-editor/react";
import type monaco from "monaco-editor";
import exampleLuaGuitar from "./exampleLua/guitar.lua?raw";
import { MdPlayArrow } from "react-icons/md";
import { css } from "@emotion/react";
import PianoWidget from "./components/PianoWidget";
import NXOTable from "./components/NXOTable";
import {
  isNXODefinition,
  type NXODefinition,
} from "./utils/validateLuaResult";

function App() {
  const editorRef = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const workerRef = useRef<Worker>(undefined);
  const [compileError, setCompileError] = useState<string | null>(null);
  const [compileResult, setCompileResult] = useState<NXODefinition | null>(
    null
  );

  const [midiStates, setMidiStates] = useState<Array<boolean>>(
    new Array(128).fill(false)
  );
  const midiStatesBackupRef = useRef<Array<boolean>>(
    new Array(128).fill(false)
  );

  const handleEditorDidMount: OnMount = (editor, monaco) => {
    editorRef.current = editor;
  };

  useEffect(() => {
    workerRef.current = new Worker(
      new URL("./workers/luaWorker.ts", import.meta.url),
      { type: "module" }
    );
    return () => workerRef.current?.terminate();
  }, []);

  useEffect(() => {
    const worker = workerRef.current;
    if (!worker) return;
    const handler = (
      e: MessageEvent<{ id: number; result?: unknown; error?: string }>
    ) => {
      if (e.data.error) {
        setCompileError(e.data.error);
        setCompileResult(null);
      } else if (isNXODefinition(e.data.result)) {
        setCompileError(null);
        setCompileResult(e.data.result);
        const win = window as object as NIHPlugWebviewWindow;
        if (typeof win.sendToPlugin === "function") {
          win.sendToPlugin({
            type: "SetNxoDefinition",
            definition: e.data.result,
          });
        }
      } else {
        setCompileError(
`
Invalid return shape.

Return value should be a lua table which is akin to the following typescript type:

{
  [frequencyMultiplier: string|number]:
    {
      v: number;
      a: number;
      d: number;
      s: number;
      r: number;
    }
}
`


        );
        setCompileResult(null);
      }
    };
    worker.addEventListener("message", handler);
    return () => worker.removeEventListener("message", handler);
  }, []);

  const [ipcReady, setIpcReady] = useState(false);

  function checkIpcReady() {
    const asModifiedWindow = window as object as NIHPlugWebviewWindow;
    if (typeof asModifiedWindow.sendToPlugin === "function") {
      setIpcReady(true);
    }
  }

  useEffect(() => {
    if (ipcReady) return;
    const interval = setInterval(checkIpcReady, 100);
    return () => clearInterval(interval);
  }, [ipcReady]);

  const [cargoPackageVersion, setCargoPackageVersion] = useState("");

  const [gain, setGain] = useState<number | null>(null);

  const incomingMessageHandlers = useMemo(() => {
    return {
      RespondCargoPackageVersion: async (payload: { version: string }) => {
        setCargoPackageVersion(payload.version);
      },
      RespondGain: async (payload: { gain: number }) => {
        setGain(payload.gain);
      },
      MidiStateUpdate: async (payload: { states: boolean[] }) => {
        if (midiStatesBackupRef.current.some((s) => s)) {
          setMidiStates(payload.states);
          midiStatesBackupRef.current = [...payload.states];
        } else {
          if (payload.states.some((s) => s)) {
            setMidiStates(payload.states);
            midiStatesBackupRef.current = [...payload.states];
          }
        }
      },
    };
  }, []) as object as Record<
    string,
    (payload: Record<string, unknown>) => void | Promise<void>
  >;

  useEffect(() => {
    (window as object as NIHPlugWebviewWindow).onPluginMessage = (
      payload: Record<string, unknown>
    ) => {
      const messageType = payload.type as keyof typeof incomingMessageHandlers;
      if (!incomingMessageHandlers[messageType]) {
        console.error(`Received unknown message type: ${messageType}`);
        return;
      }
      incomingMessageHandlers[messageType](payload as Record<string, unknown>);
    };
  }, []);

  useEffect(() => {
    if (!ipcReady) return;
    (window as object as NIHPlugWebviewWindow).sendToPlugin({
      type: "QueryCargoPackageVersion",
    });
    (window as object as NIHPlugWebviewWindow).sendToPlugin({
      type: "QueryGain",
    });
  }, [ipcReady]);

  const onGainChange = useMemo(
    () =>
      lodash.throttle(
        (v: number) => {
          (window as object as NIHPlugWebviewWindow).sendToPlugin({
            type: "SetGainDB",
            gain: v,
          });
          setGain(v);
        },
        100,
        {
          leading: true,
          trailing: true,
        }
      ),
    []
  );

  return (
    <Div
      width="100dvw"
      height="100dvh"
      display="grid"
      gridTemplateRows="auto auto 1fr auto"
      overflow="hidden"
    >
      <Div
        width="100%"
        display="flex"
        flexDirection="row"
        alignItems="center"
        justifyContent="flex-start"
        background="cornflowerblue"
        padding="0.5rem"
        gap="0.5rem"
      >
        <P
          fontSize="1rem"
          width="12rem"
          fontStyle="italic"
          fontWeight="bold"
          color="white"
        >
          {cargoPackageVersion ? `v${cargoPackageVersion}` : "..."}
        </P>
        <Div
          flex={1}
          display="flex"
          flexDirection="row"
          alignItems="center"
          justifyContent="center"
        >
          {typeof gain === "number" && (
            <Div width="100px">
              <Slider
                ariaLabelledby="gain-slider-label"
                className="horizontal-slider"
                thumbClassName="example-thumb"
                trackClassName="example-track"
                min={-30}
                max={0}
                value={gain}
                onChange={onGainChange}
                renderThumb={(props, state) => (
                  <div {...props}>
                    <div
                      style={{
                        position: "absolute",
                        top: 0,
                        bottom: 0,
                        left: 0,
                        right: 0,
                        transformOrigin: "center",
                        fontSize: "1rem",
                        color: "white",
                        fontWeight: "bold",
                        textAlign: "center",
                        transform: "translateY(1.75rem)",
                        display: "flex",
                        flexDirection: "column",
                        alignItems: "center",
                        justifyContent: "center",
                      }}
                    >
                      <div
                        style={{
                          whiteSpace: "nowrap",
                          background: "black",
                          borderRadius: "0.5rem",
                          padding: "0.5rem",
                          fontSize: "0.75rem",
                        }}
                      >
                        {state.valueNow} dB
                      </div>
                    </div>
                  </div>
                )}
              />
            </Div>
          )}
        </Div>
        <P visibility="hidden" width="12rem"></P>
      </Div>
      <Div
        width="100dvw"
        padding="0.5rem"
        background="skyblue"
        display="flex"
        flexDirection="row"
        alignItems="center"
        justifyContent="flex-start"
      >
        <H1 flex={0} fontSize="1.5rem" whiteSpace="nowrap">
          Harmonic NXO
        </H1>
        <Div flex={1}></Div>
        <Button
          flex={0}
          fontSize="1.25rem"
          padding="0.25rem"
          borderRadius="0.75rem"
          border="2px solid white"
          color="white"
          display="flex"
          flexDirection="row"
          alignItems="center"
          justifyContent="center"
          cursor="pointer"
          transformOrigin="center"
          transition="transform 0.1s ease-in-out"
          css={css`
            transform: scale(1);
            background: blue;
            &:hover {
              transform: scale(1.05);
              background: lightblue;
            }
            &:active {
              transform: scale(0.95);
              background: green;
            }
          `}
          onClick={() => {
            const code = editorRef.current?.getValue() ?? "";
            setCompileError(null);
            setCompileResult(null);
            workerRef.current?.postMessage({ id: Date.now(), code });
          }}
        >
          <Span>Compile</Span>
          <MdPlayArrow />
        </Button>
      </Div>
      <Div display="grid" gridTemplateColumns="1fr 1fr">
        <Editor
          theme="vs-dark"
          height="100%"
          defaultLanguage="lua"
          defaultValue={exampleLuaGuitar}
          onMount={handleEditorDidMount}
          options={{
            wordWrap: "on",
          }}
        />
        <Div padding="0.5rem" overflow="auto">
          {compileError && <pre style={{ color: "red" }}>{compileError}</pre>}
          {!compileError && compileResult && (
            <NXOTable nxoDefinition={compileResult} />
          )}
        </Div>
      </Div>
      {/* Piano Widget */}
      <PianoWidget midiStates={midiStates} />
    </Div>
  );
}

export default App;
