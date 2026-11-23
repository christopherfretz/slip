#include "side.h"
// digest §3.4, WP-039 item 1: a using-declaration against the entity it names: two keys.
namespace n { void f(); }
#ifdef B
using n::f;
#endif
