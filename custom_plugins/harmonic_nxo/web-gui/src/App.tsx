import { useEffect, useState } from "react";

import { Div, H1, P } from "style-props-html";

function App() {
  const [cargoPackageVersion, setCargoPackageVersion] = useState("");
  // To gain more info about caching behavior of the VST as well as session storage
  // Will track the "refresh" count in session storage by incrementing on mount
  const [refreshCount, setRefreshCount] = useState<number|null>(null)
  useEffect(() => {}, []);

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
        <P fontSize="1rem" fontStyle="italic">{cargoPackageVersion ? `v${cargoPackageVersion}`: '...' }</P>
      </Div>
      <Div width="100%" padding="0.5rem" background="skyblue">
        <H1 fontSize="1.5rem">Harmonic NXO</H1>
      </Div>
    </Div>
  );
}

export default App;
