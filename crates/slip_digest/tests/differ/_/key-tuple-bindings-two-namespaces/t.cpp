#include "side.h"
// digest §3.4, WP-039 item 1: tuple-like structured bindings of one name in two namespaces: two keys.
#include "tuple.h"
T t;
namespace m { auto [a, b] = t; }
#ifdef B
namespace n { auto [a, b] = t; }
#endif
