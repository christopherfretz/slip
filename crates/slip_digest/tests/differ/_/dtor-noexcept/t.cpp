#include "side.h"
// §11.2: a destructor is of the deferrable class; its specification is the
// resolved unit `exception_spec`, which the demand area writes.
#ifdef B
struct S { ~S() noexcept(false); };
#else
struct S { ~S(); };
#endif
