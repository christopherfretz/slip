#include "side.h"
// §10.1 AttrNode: `GSL_POINTER.flags-has-type`: the fact differs.
#ifdef B
struct [[gsl::Pointer(int)]] S {};
#else
struct [[gsl::Pointer]] S {};
#endif
