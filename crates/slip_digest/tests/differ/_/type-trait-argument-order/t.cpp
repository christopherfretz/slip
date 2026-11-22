#include "side.h"
// §7 _: `type-trait-argument-order`: the fact differs.
struct T { T(int, long); };
#ifdef B
bool v = __is_constructible(T, long, int);
#else
bool v = __is_constructible(T, int, long);
#endif
