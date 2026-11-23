#include "side.h"
// digest §3.4, WP-039 item 1: an alias template against its target: two keys.
namespace a { template <class T> struct X {}; }
#ifdef B
template <class T> using X = a::X<T>;
#endif
