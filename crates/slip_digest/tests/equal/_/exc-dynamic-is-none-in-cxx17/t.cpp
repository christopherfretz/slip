// std: c++17
// args: -Wno-dynamic-exception-spec
#include "side.h"
// digest-kinds rule `ExcSpec` `Dynamic` in C++17: `throw(int)` is canonical `None` (kiln_ast/src/types/canon.rs:657).
#ifdef B
void f() throw(int);
#else
void f() noexcept(false);
#endif
