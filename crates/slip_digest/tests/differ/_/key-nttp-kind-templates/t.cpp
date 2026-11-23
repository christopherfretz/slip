#include "side.h"
// digest §3.4, WP-039 item 1: function templates differing in the kind of a template parameter: two keys.
template <class T> void f();
#ifdef B
template <int N> void f();
#endif
