import { emphasize as __bs_external_fn46, joinLabel as __bs_external_fn47 } from "../formatters-c9889c4c3b77069b.js";
import { createMetric as __bs_external_fn42, loadMetricLabel as __bs_external_fn44, metricLabel as __bs_external_fn43, setMetricValue as __bs_external_fn45 } from "../metrics-dc132e89635c604f.js";

export function __bs_glue_fn42(...args) {
    return __bs_external_fn42(...args);
}

export function __bs_glue_fn43(...args) {
    return __bs_external_fn43(...args);
}

export function __bs_glue_fn44(...args) {
    let result;
    try {
        result = __bs_external_fn44(...args);
    } catch (e) {
        return { tag: "err", value: { message: String(e.message || e), code: 0 } };
    }

    if (result && typeof result.ok === "boolean") {
        if (result.ok === true) {
            return { tag: "ok", value: result.value };
        }
        if (result.ok === false) {
            const error = result.error || { message: "Unknown error", code: 0 };
            return { tag: "err", value: { message: error.message || "Unknown error", code: typeof error.code === "number" ? error.code : 0 } };
        }
    }

        throw new Error(
            "Invalid result wrapper from external function '__bs_glue_fn44': " +
            "expected { ok: boolean, value? } or { ok: false, error: { code, message } }"
        );
}

export function __bs_glue_fn45(...args) {
    return __bs_external_fn45(...args);
}

export function __bs_glue_fn46(...args) {
    return __bs_external_fn46(...args);
}

export function __bs_glue_fn47(...args) {
    return __bs_external_fn47(...args);
}
