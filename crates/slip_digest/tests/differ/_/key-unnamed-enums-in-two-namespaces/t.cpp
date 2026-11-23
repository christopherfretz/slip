#include "side.h"
// digest §3.4, WP-039 item 1: unnamed enumerations in two namespaces: two keys.
namespace a { enum { A }; }
#ifdef B
namespace b { enum { A }; }
#endif
