import { emphasize as __bs_external_fn182, joinLabel as __bs_external_fn183 } from "../formatters-c9889c4c3b77069b.js";
import { createMetric as __bs_external_fn178, loadMetricLabel as __bs_external_fn180, metricLabel as __bs_external_fn179, setMetricValue as __bs_external_fn181 } from "../metrics-dc132e89635c604f.js";

export function __bs_glue_fn178(...args) {
    return __bs_external_fn178(...args);
}

export function __bs_glue_fn179(...args) {
    return __bs_external_fn179(...args);
}

export function __bs_glue_fn180(...args) {
    let result;
    try {
        result = __bs_external_fn180(...args);
    } catch (e) {
        return { tag: "err", value: { bst_message_fld0: String(e.message || e), bst_code_fld1: 0 } };
    }

    if (result && typeof result.ok === "boolean") {
        if (result.ok === true) {
            return { tag: "ok", value: result.value };
        }
        if (result.ok === false) {
            const error = result.error || { message: "Unknown error", code: 0 };
            return { tag: "err", value: { bst_message_fld0: error.message || "Unknown error", bst_code_fld1: typeof error.code === "number" ? error.code : 0 } };
        }
    }

        throw new Error(
            "Invalid result wrapper from external function '__bs_glue_fn180': " +
            "expected { ok: boolean, value? } or { ok: false, error: { code, message } }"
        );
}

export function __bs_glue_fn181(...args) {
    return __bs_external_fn181(...args);
}

export function __bs_glue_fn182(...args) {
    return __bs_external_fn182(...args);
}

export function __bs_glue_fn183(...args) {
    return __bs_external_fn183(...args);
}
