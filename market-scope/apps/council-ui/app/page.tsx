/**
 * MARKETSCOPE — Council UI
 *
 * Presents scenario bands (NOT point forecasts).
 * All outputs require Human Review Required.
 *
 * Training_Gate: Human_Review_Required
 * Audit_Spec: 4b565498-9afc-4782-af4a-c6b11a5d0058
 */

"use client";

import { useState } from "react";

interface Scenario {
  scenario: string;
  label: string;
  range: number[];
  probability: string;
  note: string;
}

interface ScenarioResult {
  symbol: string;
  timestamp: string;
  data_hash: string;
  statistics: {
    mean: number;
    std: number;
    min: number;
    max: number;
    count: number;
  };
  scenarios: Scenario[];
  disclaimer: string;
  training_gate: string;
  audit_spec: string;
}

export default function CouncilUI() {
  const [symbol, setSymbol] = useState("SPX");
  const [values, setValues] = useState("");
  const [result, setResult] = useState<ScenarioResult | null>(null);
  const [loading, setLoading] = useState(false);

  const generateScenarios = async () => {
    setLoading(true);
    try {
      const valuesArray = values.split(",").map((v) => parseFloat(v.trim())).filter((v) => !isNaN(v));
      const response = await fetch("http://localhost:8081/scenarios", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ symbol, values: valuesArray }),
      });
      const data = await response.json();
      setResult(data);
    } catch (error) {
      console.error("Error:", error);
    }
    setLoading(false);
  };

  return (
    <div className="min-h-screen bg-gray-900 text-white p-8">
      <div className="max-w-4xl mx-auto">
        <h1 className="text-4xl font-bold mb-2 text-cyan-400">MARKETSCOPE</h1>
        <p className="text-gray-400 mb-8">Sovereign Market Intelligence & Research Platform</p>

        <div className="bg-gray-800 rounded-lg p-6 mb-8 border border-gray-700">
          <h2 className="text-xl font-semibold mb-4 text-orange-400">Scenario Generator</h2>
          <div className="grid grid-cols-2 gap-4 mb-4">
            <div>
              <label className="block text-sm text-gray-400 mb-1">Symbol</label>
              <input
                type="text"
                value={symbol}
                onChange={(e) => setSymbol(e.target.value)}
                className="w-full bg-gray-700 rounded px-3 py-2 text-white"
              />
            </div>
            <div>
              <label className="block text-sm text-gray-400 mb-1">Values (comma-separated)</label>
              <input
                type="text"
                value={values}
                onChange={(e) => setValues(e.target.value)}
                placeholder="100, 101, 99, 102, 98"
                className="w-full bg-gray-700 rounded px-3 py-2 text-white"
              />
            </div>
          </div>
          <button
            onClick={generateScenarios}
            disabled={loading}
            className="bg-cyan-600 hover:bg-cyan-700 px-6 py-2 rounded font-semibold disabled:opacity-50"
          >
            {loading ? "Generating..." : "Generate Scenario Bands"}
          </button>
        </div>

        {result && (
          <div className="bg-gray-800 rounded-lg p-6 border border-gray-700">
            <div className="flex justify-between items-center mb-4">
              <h2 className="text-xl font-semibold text-cyan-400">{result.symbol} — Scenario Bands</h2>
              <span className="text-xs text-yellow-400 bg-yellow-900/30 px-2 py-1 rounded">
                HUMAN REVIEW REQUIRED
              </span>
            </div>

            <div className="bg-red-900/30 border border-red-700 rounded p-3 mb-4">
              <p className="text-red-400 text-sm font-semibold">
                ⚠️ {result.disclaimer}
              </p>
            </div>

            <div className="grid grid-cols-4 gap-4 mb-6">
              <div className="bg-gray-700 rounded p-3">
                <p className="text-xs text-gray-400">Mean</p>
                <p className="text-lg font-mono">{result.statistics.mean.toFixed(2)}</p>
              </div>
              <div className="bg-gray-700 rounded p-3">
                <p className="text-xs text-gray-400">Std Dev</p>
                <p className="text-lg font-mono">{result.statistics.std.toFixed(2)}</p>
              </div>
              <div className="bg-gray-700 rounded p-3">
                <p className="text-xs text-gray-400">Min</p>
                <p className="text-lg font-mono">{result.statistics.min.toFixed(2)}</p>
              </div>
              <div className="bg-gray-700 rounded p-3">
                <p className="text-xs text-gray-400">Max</p>
                <p className="text-lg font-mono">{result.statistics.max.toFixed(2)}</p>
              </div>
            </div>

            <div className="space-y-3">
              {result.scenarios.map((scenario, i) => (
                <div key={i} className="bg-gray-700/50 rounded p-4 border-l-4 border-cyan-500">
                  <div className="flex justify-between items-center">
                    <span className="font-semibold">{scenario.label}</span>
                    <span className="text-xs text-gray-400">{scenario.probability}</span>
                  </div>
                  <p className="text-cyan-300 font-mono mt-1">
                    [{scenario.range[0].toFixed(2)} — {scenario.range[1].toFixed(2)}]
                  </p>
                  <p className="text-xs text-yellow-500 mt-1">{scenario.note}</p>
                </div>
              ))}
            </div>

            <div className="mt-4 text-xs text-gray-500">
              <p>Data Hash: {result.data_hash}</p>
              <p>Audit Spec: {result.audit_spec}</p>
              <p>Timestamp: {result.timestamp}</p>
            </div>
          </div>
        )}

        <div className="mt-8 text-center text-xs text-gray-600">
          <p>Training_Gate: Human_Review_Required</p>
          <p>Audit_Spec: 4b565498-9afc-4782-af4a-c6b11a5d0058</p>
          <p>PROHIBITED_ACTION: financial_advice</p>
        </div>
      </div>
    </div>
  );
}
