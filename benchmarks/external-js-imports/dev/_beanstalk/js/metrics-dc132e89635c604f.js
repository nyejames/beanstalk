import { bstOk, bstErr } from "@beanstalk/runtime";

/**
 * @bst.opaque MetricHandle
 */

/**
 * @bst.sig create_metric |name String, value Int| -> MetricHandle
 */
export function createMetric(name, value) {
    return { name, value };
}

/**
 * @bst.sig metric_label |metric MetricHandle| -> String
 */
export function metricLabel(metric) {
    return `${metric.name}:${metric.value}`;
}

/**
 * @bst.sig set_metric_value |this ~MetricHandle, value Int|
 */
export function setMetricValue(metric, value) {
    metric.value = value;
}

/**
 * @bst.sig load_metric_label |id String| -> String, Error!
 */
export function loadMetricLabel(id) {
    if (id === "") {
        return bstErr(404, "Missing metric id");
    }

    return bstOk(`metric:${id}`);
}
