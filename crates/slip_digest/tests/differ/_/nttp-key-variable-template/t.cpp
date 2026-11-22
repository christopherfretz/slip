#include "side.h"
// WP-028: `vt<1>` and `vt<2>` are different specialisations of a variable template (a value argument in the key). Written as explicit specialisations, which are declared units: an implicit one is a derived unit, and `int* p = &vt<1>;` shows nothing while expressions are pending.
template <int N> int vt = N;
#ifdef B
template <> int vt<2> = 0;
#else
template <> int vt<1> = 0;
#endif
