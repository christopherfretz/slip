#include "side.h"
// WP-035, digest-kinds §7 `_` `init-list-range-against-written-out`: one node in three slots (one run) against three nodes written out (three runs).
#ifdef B
int v[3] = {7, 7, 7};
#else
int v[3] = {[0 ... 2] = 7};
#endif
