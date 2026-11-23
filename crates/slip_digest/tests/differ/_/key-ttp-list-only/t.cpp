#include "side.h"
// digest §3.4, WP-039 item 1: function templates differing only in a template template parameter's list: two keys.
template <class> struct X1 {};
template <template <class> class TT> void f();
#ifdef B
template <template <class, class> class TT> void f();
#endif
