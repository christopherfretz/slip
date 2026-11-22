#include "side.h"
// §10.1 AttrKind: `NO_INSTRUMENT_FUNCTION`: the attribute on one side only.
#ifdef B
__attribute__((no_instrument_function)) void f();
#else
void f();
#endif
