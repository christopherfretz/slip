#include "side.h"
// digest §3.4, WP-039 item 1: an explicitly specialised member: two keys.
template <class T> struct A { void f(); };
#ifdef B
template <> void A<int>::f();
#endif
