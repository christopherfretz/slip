#include "side.h"
// §10.1 AttrNode: `GSL_POINTER.b`: the fact differs.
#ifdef B
struct [[gsl::Pointer(long)]] S {};
#else
struct [[gsl::Pointer(int)]] S {};
#endif
