// std: c++17
// args: -Wno-dynamic-exception-spec
#include "side.h"
// §4.6 ExcSpec `Dynamic.0`: the types.
#ifdef B
void f() throw(long);
#else
void f() throw(int);
#endif
