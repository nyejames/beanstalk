export function bstOk(value) {
    return { ok: true, value: value };
}

export function bstErr(code, message) {
    return { ok: false, error: { code, message } };
}
