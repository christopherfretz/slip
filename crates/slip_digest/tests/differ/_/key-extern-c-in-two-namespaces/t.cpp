#include "side.h"
// digest §3.4, WP-039 item 1: one C function declared in two namespaces: one entity: two keys.
namespace a { extern "C" int f(); }
#ifdef B
namespace b { extern "C" int f(); }
#endif
