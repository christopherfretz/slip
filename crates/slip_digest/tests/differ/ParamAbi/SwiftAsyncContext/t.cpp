#include "side.h"
// §4.5 ParamAbi `SwiftAsyncContext`: against `swift_context`.
#ifdef B
__attribute__((swiftcall)) void f(void **a, void **c __attribute__((swift_context)));
#else
__attribute__((swiftcall)) void f(void **a, void **c __attribute__((swift_async_context)));
#endif
