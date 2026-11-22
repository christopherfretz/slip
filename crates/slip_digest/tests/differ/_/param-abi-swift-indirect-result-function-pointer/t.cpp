#include "side.h"
// §4.5 ParamAbi `SwiftIndirectResult` against `SwiftAsyncContext`, in a function pointer's type: no declaration attribute writes it (FunctionProto `param_abi_list`, `types.rs`).
#ifdef B
__attribute__((swiftcall)) void (*p)(void **a __attribute__((swift_async_context)), void **c __attribute__((swift_context)));
#else
__attribute__((swiftcall)) void (*p)(void **a __attribute__((swift_indirect_result)), void **c __attribute__((swift_context)));
#endif
