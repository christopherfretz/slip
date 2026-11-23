#include "side.h"
// digest §3.4, WP-039 item 1: function templates differing only in an array bound expression: two keys.
template <int N> void f(int (&)[N]);
#ifdef B
template <int N> void f(int (&)[N + 1]);
#endif
