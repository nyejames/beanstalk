// TOKEN LOGGING MACROS
#[macro_export]
#[cfg(feature = "show_tokens")]
macro_rules! token_log {
    ($($arg:tt)*) => {
        saying::say!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(feature = "show_tokens"))]
macro_rules! token_log {
    ($($arg:tt)*) => {
        // Nothing
    };
}

// Extra timer logging
#[macro_export]
#[cfg(feature = "detailed_timers")]
macro_rules! timer_log {
    ($time:expr, $msg:expr) => {
        saying::say!($msg, Green #$time.elapsed());
    };
}

#[macro_export]
#[cfg(not(feature = "detailed_timers"))]
macro_rules! timer_log {
    ($time:expr, $msg:expr) => {
        // Nothing
    };
}

// Headers Logging
#[macro_export]
#[cfg(feature = "show_headers")]
macro_rules! header_log {
    ($header:expr) => {
        saying::say!("\n", $header.to_string());
    };
}

#[macro_export]
#[cfg(not(feature = "show_headers"))]
macro_rules! header_log {
    ($header:expr) => {
        // Nothing
    };
}

// AST LOGGING MACROS
#[macro_export]
#[cfg(feature = "show_ast")]
macro_rules! ast_log {
    ($($arg:tt)*) => {
        saying::say!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(feature = "show_ast"))]
macro_rules! ast_log {
    ($($arg:tt)*) => {
        // Nothing
    };
}

// EVAL LOGGING MACROS
#[macro_export]
#[cfg(feature = "show_eval")]
macro_rules! eval_log {
    ($($arg:tt)*) => {
        saying::say!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(feature = "show_eval"))]
macro_rules! eval_log {
    ($($arg:tt)*) => {
        // Nothing
    };
}

// CODEGEN LOGGING MACROS
#[macro_export]
#[cfg(feature = "show_codegen")]
macro_rules! codegen_log {
    ($($arg:tt)*) => {
        saying::say!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(feature = "show_codegen"))]
macro_rules! codegen_log {
    ($($arg:tt)*) => {
        // Nothing
    };
}

// HIR LOGGING MACROS
#[macro_export]
#[cfg(feature = "show_hir")]
macro_rules! hir_log {
    ($($arg:tt)*) => {
        saying::say!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(feature = "show_hir"))]
macro_rules! hir_log {
    ($($arg:tt)*) => {
        // Nothing
    };
}

// BORROW CHECKER LOGGING MACROS
#[macro_export]
#[cfg(feature = "show_borrow_checker")]
macro_rules! borrow_log {
    ($($arg:tt)*) => {
        saying::say!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(feature = "show_borrow_checker"))]
macro_rules! borrow_log {
    ($($arg:tt)*) => {
        // Nothing
    };
}
