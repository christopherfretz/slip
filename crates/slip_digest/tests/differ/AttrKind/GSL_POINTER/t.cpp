#include "side.h"
// §10.1 AttrKind: `GSL_POINTER`: the attribute on one side only.
#ifdef B
struct [[gsl::Pointer]] S {};
#else
struct S {};
#endif
