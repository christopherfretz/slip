#include "side.h"
// §10.1 AttrNode: `GSL_OWNER.b`: the fact differs.
#ifdef B
struct [[gsl::Owner(long)]] S {};
#else
struct [[gsl::Owner(int)]] S {};
#endif
