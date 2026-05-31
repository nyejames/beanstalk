/**
 * @bst.sig emphasize |text String| -> String
 */
export const emphasize = (text) => {
    return `**${text}**`;
};

/**
 * @bst.sig join_label |prefix String, body String| -> String
 */
export function joinLabel(prefix, body) {
    return `${prefix}: ${body}`;
}
