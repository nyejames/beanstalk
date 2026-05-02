-- Test that internal semicolons in trait method requirements are rejected
Drawable must:
    draw |this|;
    move |this, x Int, y Int|;
;
