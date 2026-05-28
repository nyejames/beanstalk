//! `@core/prelude` package registration.
//!
//! WHAT: registers the bare prelude symbols that are available without
//! explicit imports in every module.
//! WHY: the prelude defines the minimal universal surface; builders must
//! provide it, but the actual implementations live in specific core packages
//! such as `@core/io`.

use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::external_packages::{
    ExternalFunctionId, ExternalSymbolId, ExternalTypeId, IO_FUNC_NAME, IO_TYPE_NAME,
};

pub fn register_core_prelude(registry: &mut ExternalPackageRegistry) {
    registry
        .register_prelude_symbol(
            IO_FUNC_NAME,
            ExternalSymbolId::Function(ExternalFunctionId::Io),
        )
        .expect("prelude registration should not collide");

    registry
        .register_prelude_symbol(IO_TYPE_NAME, ExternalSymbolId::Type(ExternalTypeId(0)))
        .expect("prelude registration should not collide");
}
