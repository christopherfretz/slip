// std: c++26
#include "side.h"
// §9.5 TemplateTables `pack_index_origin`: the selected element of a
// substituted `pack...[i]` against the same expression written plainly.
#ifdef B
template <int... N> int f() { return 1; }
int x = f<1>();
#else
template <int... N> int f() { return N...[0]; }
int x = f<1>();
#endif
