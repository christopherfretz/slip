#include "side.h"
// §5.3 DeclNode `ty-exception-spec`: not in the key, in the unit.
#ifdef B
void f() noexcept;
#else
void f();
#endif
