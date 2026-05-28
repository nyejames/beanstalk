/**
 * @bst.opaque Canvas
 * @bst.opaque Canvas2d
 */

import { bstOk, bstErr } from "@beanstalk/runtime";

/**
 * @bst.sig get_canvas |id String| -> Canvas, Error!
 */
export function getCanvas(id) {
    const canvas = document.getElementById(id);

    if (!canvas || canvas.tagName !== "CANVAS") {
        return bstErr(404, "Canvas element not found");
    }

    return bstOk(canvas);
}

/**
 * @bst.sig context_2d |canvas Canvas| -> Canvas2d, Error!
 */
export function context2d(canvas) {
    const ctx = canvas.getContext("2d");

    if (!ctx) {
        return bstErr(500, "Could not get 2D context");
    }

    return bstOk(ctx);
}

/**
 * @bst.sig clear_rect |this ~Canvas2d, x Float, y Float, width Float, height Float|
 */
export function clearRect(ctx, x, y, width, height) {
    ctx.clearRect(x, y, width, height);
}

/**
 * @bst.sig fill_rect |this ~Canvas2d, x Float, y Float, width Float, height Float|
 */
export function fillRect(ctx, x, y, width, height) {
    ctx.fillRect(x, y, width, height);
}

/**
 * @bst.sig set_fill_style |this ~Canvas2d, color String|
 */
export function setFillStyle(ctx, color) {
    ctx.fillStyle = color;
}

/**
 * @bst.sig begin_path |this ~Canvas2d|
 */
export function beginPath(ctx) {
    ctx.beginPath();
}

/**
 * @bst.sig move_to |this ~Canvas2d, x Float, y Float|
 */
export function moveTo(ctx, x, y) {
    ctx.moveTo(x, y);
}

/**
 * @bst.sig line_to |this ~Canvas2d, x Float, y Float|
 */
export function lineTo(ctx, x, y) {
    ctx.lineTo(x, y);
}

/**
 * @bst.sig stroke |this ~Canvas2d|
 */
export function stroke(ctx) {
    ctx.stroke();
}
