#include "side.h"
// §4.5 ParamAbi `SwiftIndirectResult`: against an ordinary parameter.
#ifdef B
__attribute__((swiftcall)) void f(void **a, void **c __attribute__((swift_context)));
#else
__attribute__((swiftcall)) void f(void **a __attribute__((swift_indirect_result)), void **c __attribute__((swift_context)));
#endif
