#include "side.h"
// §4.5 ParamAbi `SwiftErrorResult`: against an ordinary parameter.
#ifdef B
__attribute__((swiftcall)) void f(void **c __attribute__((swift_context)), void **e);
#else
__attribute__((swiftcall)) void f(void **c __attribute__((swift_context)), void **e __attribute__((swift_error_result)));
#endif
