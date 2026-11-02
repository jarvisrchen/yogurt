/* tslint:disable */
/* eslint-disable */

export class Enhanced {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    enriched_md: string;
    template: string;
}

/**
 * The stored document for the model's whole reply, as the Mac's `/enhance` builds it.
 */
export function finish_enhance(notes_md: string, transcript_json: string, llm_output: string, template?: string | null): Enhanced;

/**
 * The document so far, for a live preview while the reply streams.
 */
export function preview(llm_output: string): string;

/**
 * The chat messages the Mac sends for this enhance, as JSON. `template` is a format id, or none or `auto` to let the model pick.
 */
export function render_enhance(notes_md: string, transcript_json: string, template?: string | null): string;

/**
 * Whether the meeting is an accidental start/stop, which the Mac does not enhance.
 */
export function too_short(notes_md: string, transcript_json: string): boolean;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_enhanced_free: (a: number, b: number) => void;
    readonly __wbg_get_enhanced_enriched_md: (a: number, b: number) => void;
    readonly __wbg_get_enhanced_template: (a: number, b: number) => void;
    readonly __wbg_set_enhanced_enriched_md: (a: number, b: number, c: number) => void;
    readonly __wbg_set_enhanced_template: (a: number, b: number, c: number) => void;
    readonly finish_enhance: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => void;
    readonly preview: (a: number, b: number, c: number) => void;
    readonly render_enhance: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly too_short: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
