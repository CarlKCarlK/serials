import init, { Machine, SpaceByTimeMachine } from './pkg/busy_beaver_blaze.js';

// Initialize WASM once
let wasmReady = init();

// Persistent state for live recoloring
let space_by_time_machine = null;              // SpaceByTimeMachine
let currentColors = null;     // Uint8Array(15) - always 5 colors
let stopRequested = false;    // flag to stop stepping but keep machine
let runCounter = 0;           // increment per start to tag logs

function normalizeError(err) {
    let message = 'Unknown worker error';
    let detail = '';
    let kind = 'runtime';

    if (err instanceof Error) {
        message = err.message || err.toString();
        detail = err.stack || '';
        const name = (err.name || '').toLowerCase();
        if (name.includes('security')) {
            kind = 'worker-blocked';
        } else if (name.includes('wasm') || name.includes('webassembly')) {
            kind = 'wasm-blocked';
        } else if (name) {
            kind = name;
        }
    } else if (err && typeof err === 'object') {
        if (typeof err.message === 'string' && err.message) {
            message = err.message;
        } else if (typeof err.error === 'string') {
            message = err.error;
        } else if (err.error && typeof err.error.message === 'string') {
            message = err.error.message;
        } else if (typeof err.type === 'string') {
            message = err.type;
        } else if (typeof err.toString === 'function') {
            message = err.toString();
        }

        if (typeof err.stack === 'string') {
            detail = err.stack;
        } else if (typeof err.detail === 'string') {
            detail = err.detail;
        } else if (err.error && typeof err.error.stack === 'string') {
            detail = err.error.stack;
        }

        if (typeof err.kind === 'string') {
            kind = err.kind;
        } else if (typeof err.category === 'string') {
            kind = err.category;
        } else if (typeof err.name === 'string') {
            kind = err.name;
        } else if (typeof err.type === 'string') {
            kind = err.type;
        }
    } else if (typeof err === 'string') {
        message = err;
    } else {
        message = String(err);
    }

    const lower = `${message} ${detail}`.toLowerCase();
    const normalizedKind = (kind || '').toLowerCase();
    let category = normalizedKind || 'runtime';

    if (
        lower.includes('webassembly') ||
        lower.includes('wasm') ||
        lower.includes('instantiate') ||
        lower.includes('compile') ||
        lower.includes('module script')
    ) {
        category = 'wasm-blocked';
    } else if (
        lower.includes('worker') && lower.includes('security')
    ) {
        category = 'worker-blocked';
    } else if (
        lower.includes('sharedarraybuffer') ||
        lower.includes('cross-origin-isolated')
    ) {
        category = 'worker-blocked';
    } else if (normalizedKind.includes('security')) {
        category = 'worker-blocked';
    }

    return { message, detail, kind: category };
}

function postWorkerError(source, err) {
    const info = normalizeError(err);
    console.error('[worker] error', source, info.message, info.detail);
    self.postMessage({
        success: false,
        error: info.message,
        detail: info.detail,
        kind: info.kind,
        source,
    });
}

function formatPalette(bytes) {
    if (!bytes || bytes.length !== 15) return '(invalid)';
    const toHex = (v) => v.toString(16).padStart(2, '0');
    const out = [];
    for (let i = 0; i < 5; i++) {
        const r = bytes[i * 3], g = bytes[i * 3 + 1], b = bytes[i * 3 + 2];
        out.push(`#${toHex(r)}${toHex(g)}${toHex(b)}`);
    }
    return out.join(', ');
}

function defaultPaletteBytes(isDark) {
    if (isDark) {
        return new Uint8Array([
            255, 255, 255, // white (symbol 0)
            0, 0, 0,       // black (symbol 1)
            128, 128, 128, // 50% gray (symbol 2)
            64, 64, 64,    // 25% gray (symbol 3)
            192, 192, 192  // 75% gray (symbol 4)
        ]);
    }
    return new Uint8Array([
        255, 255, 255, // white (symbol 0)
        255, 165, 0,   // orange (symbol 1)
        255, 255, 0,   // yellow (symbol 2)
        255, 0, 255,   // magenta (symbol 3)
        0, 255, 255    // cyan (symbol 4)
    ]);
}

async function renderAndPost(intermediate) {
    if (!space_by_time_machine || !currentColors) return;
    try {
        const png = space_by_time_machine.to_png(currentColors);
        self.postMessage({
            success: true,
            intermediate,
            png_data: png,
            step_count: space_by_time_machine.step_count(),
            ones_count: space_by_time_machine.count_nonblanks(),
            is_halted: space_by_time_machine.is_halted()
        });
    } catch (e) {
        postWorkerError('render', e);
    }
}

self.onmessage = async function (e) {
    await wasmReady;
    const msg = e.data || {};
    const type = msg.type || 'start';

    try {
        if (type === 'start') {
            const { programText, goal_x, goal_y, early_stop, binning, darkMode, colorsBytes } = msg;
            const runId = ++runCounter;

            // Normalize to fixed 5-color palette (15 bytes) and clone to avoid aliasing
            currentColors = (colorsBytes && colorsBytes.length === 15)
                ? new Uint8Array(colorsBytes)
                : defaultPaletteBytes(!!darkMode);

            // Debug: log palette used for this run
            console.log('[worker] run #%d start colors (dark=%s): %s', runId, !!darkMode, formatPalette(currentColors));

            // Create/replace the machine
            space_by_time_machine = new SpaceByTimeMachine(programText, goal_x, goal_y, binning, 0n);
            stopRequested = false;

            const run_for_seconds = 0.1;
            while (true) {
                if (stopRequested || !space_by_time_machine.step_for_secs(run_for_seconds, early_stop, 10_000n)) {
                    break;
                }
                await renderAndPost(true);
                // Yield to event loop so we can process incoming 'colors' messages
                await new Promise((resolve) => setTimeout(resolve, 0));
            }

            // Final frame
            await renderAndPost(false);
            console.log('[worker] run #%d final frame posted', runId);
            return;
        }

        if (type === 'colors') {
            const { colorsBytes, darkMode } = msg;
            // Update colors (keep 5-color size) and clone to avoid aliasing
            currentColors = (colorsBytes && colorsBytes.length === 15)
                ? new Uint8Array(colorsBytes)
                : (currentColors || defaultPaletteBytes(!!darkMode));
            console.log('[worker] update colors (dark=%s): %s', !!darkMode, formatPalette(currentColors));
            // Re-render instantly
            await renderAndPost(true);
            return;
        }

        if (type === 'stop') {
            // Stop stepping but keep machine for recoloring
            stopRequested = true;
            self.postMessage({ success: true, intermediate: false, stopped: true });
            return;
        }

        postWorkerError('protocol', { message: `Unknown message type: ${type}`, kind: 'protocol' });
    } catch (err) {
        postWorkerError(type, err);
    }
};
