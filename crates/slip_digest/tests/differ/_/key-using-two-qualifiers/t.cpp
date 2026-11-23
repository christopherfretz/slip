#include "side.h"
// digest §3.4, WP-039 item 1: two using-declarations of one name: two keys.
namespace a { void f(); } namespace b { void f(int); }
using a::f;
#ifdef B
using b::f;
#endif
