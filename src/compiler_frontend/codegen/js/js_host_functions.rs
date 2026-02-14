pub(crate) use crate::compiler_frontend::host_functions::registry::HostFunctionId;

pub fn get_host_function_str(id: HostFunctionId) -> &'static str {
    match id {
        HostFunctionId::Io => "console.log",
        _ => "",
    }
}
