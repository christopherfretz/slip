#include "side.h"
// digest §3.4, WP-039 item 1: unnamed namespaces in two namespaces: two keys.
namespace a { namespace { int x; } }
#ifdef B
namespace b { namespace { int x; } }
#endif
