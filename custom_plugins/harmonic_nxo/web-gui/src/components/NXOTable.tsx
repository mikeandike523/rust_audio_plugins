import { Fragment } from "react";
import { Div, H1 } from "style-props-html";

import { css } from "@emotion/react";
import { type NXODefinition } from "../utils/validateLuaResult";

interface NXOTableProps {
  nxoDefinition: NXODefinition;
  precision?: number;
}

export default function NXOTable({ nxoDefinition, precision = 2 }: NXOTableProps) {
  const rows = Object.entries(nxoDefinition);

  const headerCell = css`
    text-align: center;
    font-size: 1rem;
    font-weight: bold;
    border: 1px solid #ccc;
    background-color: #072ba1ff; /* Dark purple */
    color: white;
  `;

  const cell = (isEven: boolean) => css`
    text-align: center;
    font-size: 1rem;
    border: 1px solid #ccc;
    background-color: ${isEven ? "#f5faff" : "transparent"};
  `;

  return (
    <Div
      width="100%"
      display="grid"
      gridTemplateColumns="repeat(6, 1fr)"
      border="1px solid #ccc"
    >
      {["Freq Mul", "V (pk)", "A (s)", "D (s)", "S (pk)", "R (s)"].map((header) => (
        <H1 key={header} css={headerCell}>
          {header}
        </H1>
      ))}

      {rows.map(([freqMul, { v, a, d, s, r }], idx) => {
        const isEven = idx % 2 === 1;
        return (
          <Fragment key={freqMul}>
            <Div css={cell(isEven)}>{+(Number(freqMul).toFixed(precision))}</Div>
            <Div css={cell(isEven)}>{+(v.toFixed(precision))}</Div>
            <Div css={cell(isEven)}>{+(a.toFixed(precision))}</Div>
            <Div css={cell(isEven)}>{+(d.toFixed(precision))}</Div>
            <Div css={cell(isEven)}>{+(s.toFixed(precision))}</Div>
            <Div css={cell(isEven)}>{+(r.toFixed(precision))}</Div>
          </Fragment>
        );
      })}
    </Div>
  );
}
