#include "side.h"
// digest §3.4, WP-039 item 1: two tuple-like structured bindings in one namespace: two keys.
#include "tuple.h"
T t;
auto [a, b] = t;
#ifdef B
auto [c, d] = t;
#endif
