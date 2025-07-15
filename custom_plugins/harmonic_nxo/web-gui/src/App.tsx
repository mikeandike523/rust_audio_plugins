import { useEffect, useMemo, useState } from "react";
import { Div, H1, P } from "style-props-html";
import Slider from "react-slider";
import { type NIHPlugWebviewWindow } from "./nih-plug-webview-window";
import "../styles/sliders.css";
import lodash from "lodash";

function App() {
  const [cargoPackageVersion, setCargoPackageVersion] = useState("");

  const [gain, setGain] = useState<number | null>(null);

  const incomingMessageHandlers = useMemo(() => {
    return {
      SetCargoPackageVersion: async (payload: { version: string }) => {
        setCargoPackageVersion(payload.version);
      },
      SetInitialGain: async (payload: { gain: number }) => {
        setGain(payload.gain);
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
    (window as object as NIHPlugWebviewWindow).sendToPlugin({
      type: "Init",
    });
  }, []);

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
    <Div width="100dvw" height="100dvh">
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
      <Div width="100%" padding="0.5rem" background="skyblue">
        <H1 fontSize="1.5rem">Harmonic NXO</H1>
      </Div>
    </Div>
  );
}

export default App;
