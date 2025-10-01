

// TOKEN LOGGING MACROS
#[macro_export]
#[cfg(feature = "show_tokens")]
macro_rules! token_log {
    ($token:expr) => {
        eprintln!("{}", $token.to_string())
    };
}

#[macro_export]
#[cfg(not(feature = "show_tokens"))]
macro_rules! token_log {
    ($tokens:expr) => {
        // Nothing
    };
}

// Extra timer logging
#[macro_export]
#[cfg(feature = "detailed_timers")]
macro_rules! timer_log {
    ($time:expr, $msg:expr) => {
        print!("{}", $msg);
        green_ln!("{:?}", $time.elapsed());
    };
}

#[macro_export]
#[cfg(not(feature = "detailed_timers"))]
macro_rules! timer_log {
    ($time:expr, $msg:expr) => {
        // Nothing
    };
}

// AST LOGGING MACROS
#[macro_export]
#[cfg(feature = "verbose_ast_logging")]
macro_rules! ast_log {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(feature = "verbose_ast_logging"))]
macro_rules! ast_log {
    ($($arg:tt)*) => {
        // Nothing
    };
}

// EVAL LOGGING MACROS
#[macro_export]
#[cfg(feature = "verbose_eval_logging")]
macro_rules! eval_log {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(feature = "verbose_eval_logging"))]
macro_rules! eval_log {
    ($($arg:tt)*) => {
        // Nothing
    };
}

// IR LOGGING MACROS
#[macro_export]
#[cfg(feature = "verbose_ir_logging")]
macro_rules! ir_log {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(feature = "verbose_ir_logging"))]
macro_rules! ir_log {
    ($($arg:tt)*) => {
        // Nothing
    };
}

// CODEGEN LOGGING MACROS
#[macro_export]
#[cfg(feature = "verbose_codegen_logging")]
macro_rules! codegen_log {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(feature = "verbose_codegen_logging"))]
macro_rules! codegen_log {
    ($($arg:tt)*) => {
        // Nothing
    };
}


