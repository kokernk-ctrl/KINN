/**
 * Runtime envelopes posted by previewed dynamic apps (0.4.0 S6, C.7).
 *
 * kinnrt.js reports Kin-level runtime errors (and forwards `console.*`
 * output) to its embedding parent through postMessage; these guards and
 * converters turn the raw messages into panel-ready problems so both
 * Studio variants consume them through one shared shape.
 */

export interface KinnRuntimeErrorEnvelope {
  kinnRuntimeError: {
    code: string;
    message: string;
    kinFile: string;
    kinLine: number;
  };
}

export interface KinnConsoleEnvelope {
  kinnConsole: {
    level: 'log' | 'info' | 'warn' | 'error';
    text: string;
  };
}

/** A preview message: navigation, a runtime error, or a console line. */
export type PreviewMessage =
  | { kinnPreviewNav?: boolean; page?: string }
  | KinnRuntimeErrorEnvelope
  | KinnConsoleEnvelope;

export function isRuntimeErrorEnvelope(d: unknown): d is KinnRuntimeErrorEnvelope {
  const m = d as KinnRuntimeErrorEnvelope;
  return !!m && !!m.kinnRuntimeError && typeof m.kinnRuntimeError.code === 'string';
}

export function isConsoleEnvelope(d: unknown): d is KinnConsoleEnvelope {
  const m = d as KinnConsoleEnvelope;
  return !!m && !!m.kinnConsole && typeof m.kinnConsole.text === 'string';
}

/** The runtime error as a problems-panel entry (jumpable to its line). */
export function runtimeErrorProblem(e: KinnRuntimeErrorEnvelope): {
  severity: 'error';
  code: string;
  message: string;
  line: number;
  col: number;
  file?: string;
} {
  return {
    severity: 'error',
    code: e.kinnRuntimeError.code,
    message: e.kinnRuntimeError.message,
    line: e.kinnRuntimeError.kinLine,
    col: 1,
    file: e.kinnRuntimeError.kinFile || undefined,
  };
}
