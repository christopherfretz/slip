#include "side.h"
// §5.4 VarSpec `0-placeholder` (out, Kiln WP-3583): a type deduced from `auto` against the same type written.
#ifdef B
auto v = 1;
const auto &r = v;
#else
int v = 1;
const int &r = v;
#endif
