#include "side.h"
// §4.5 ParamAbi `Ordinary`: against `swift_indirect_result` on the same parameter.
#ifdef B
__attribute__((swiftcall)) void f(void **i __attribute__((swift_indirect_result)), void **a __attribute__((swift_indirect_result)), void **c __attribute__((swift_context)));
#else
__attribute__((swiftcall)) void f(void **i __attribute__((swift_indirect_result)), void **a, void **c __attribute__((swift_context)));
#endif
