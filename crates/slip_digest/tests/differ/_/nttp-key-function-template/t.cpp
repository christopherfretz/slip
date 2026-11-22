#include "side.h"
// WP-028: `ft<1>` and `ft<2>` are different specialisations of a function template named with a value argument (a value in the key). Written as explicit specialisations, which are declared units.
template <int N> void ft() {}
#ifdef B
template <> void ft<2>() {}
#else
template <> void ft<1>() {}
#endif
