#include "side.h"
// §10.1 AttrNode: `GSL_OWNER.flags-has-type`: the fact differs.
#ifdef B
struct [[gsl::Owner(int)]] S {};
#else
struct [[gsl::Owner]] S {};
#endif
