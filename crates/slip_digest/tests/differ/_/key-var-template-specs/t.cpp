#include "side.h"
// digest §3.4, WP-039 item 1: variable template specialisations: two keys.
template <class T> int v = 0;
template <> int v<int> = 1;
#ifdef B
template <> int v<long> = 2;
#endif
