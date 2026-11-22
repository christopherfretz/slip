#include "side.h"
// §5.4 ParamData `flags-pack`: the key changes too.
#ifdef B
template <class... T> void f(T... a);
#else
template <class T> void f(T a);
#endif
