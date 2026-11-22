#include "side.h"
// §10.1 AttrKind: `GSL_OWNER`: the attribute on one side only.
#ifdef B
struct [[gsl::Owner]] S {};
#else
struct S {};
#endif
