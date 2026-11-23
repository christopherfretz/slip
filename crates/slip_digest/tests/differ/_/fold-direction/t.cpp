#include "side.h"
// §4.4, §7 _ `fold-direction`: `(... + ns)` against `(ns + ...)`.
#ifdef B
template<int... Ns> int v = (Ns + ...);
#else
template<int... Ns> int v = (... + Ns);
#endif
