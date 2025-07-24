import React from "react";
import { type HarmonicResult } from "../utils/validateLuaResult";

interface HarmonicsTableProps {
  result: HarmonicResult;
}

function formatHeader(name: string): string {
  if (/db$/i.test(name)) {
    return `${name} (dB)`;
  }
  if (/seconds?$|sec$/i.test(name)) {
    return `${name} (s)`;
  }
  return name;
}

export default function HarmonicsTable({ result }: HarmonicsTableProps) {
  const harmonicIndices = Object.keys(result)
    .map((k) => Number(k))
    .sort((a, b) => a - b);

  if (harmonicIndices.length === 0) return null;

  // Determine all columns present in the result
  const columns = Array.from(
    new Set(
      harmonicIndices.flatMap((idx) => Object.keys(result[idx]))
    )
  );

  return (
    <table
      style={{
        borderCollapse: "collapse",
        width: "100%",
        tableLayout: "auto",
      }}
    >
      <thead>
        <tr>
          <th style={{ border: "1px solid #ccc", padding: "0.25rem" }}>Harmonic</th>
          {columns.map((col) => (
            <th
              key={col}
              style={{ border: "1px solid #ccc", padding: "0.25rem" }}
            >
              {formatHeader(col)}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {harmonicIndices.map((idx) => (
          <tr key={idx}>
            <td style={{ border: "1px solid #ccc", padding: "0.25rem" }}>{idx}</td>
            {columns.map((col) => (
              <td
                key={col}
                style={{ border: "1px solid #ccc", padding: "0.25rem" }}
              >
                {String((result[idx] as Record<string, unknown>)[col] ?? "")}
              </td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  );
}
