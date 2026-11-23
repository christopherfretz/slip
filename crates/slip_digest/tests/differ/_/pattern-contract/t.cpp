#include "side.h"
// std: c++26
// WP-124 item 4 (Kiln WP-3027 "Reachable"): changed in a template nothing instantiates; the AST holds it.
template<class T> int f(T x)
#ifdef B
  pre(x > 1)
#else
  pre(x > 0)
#endif
{ return 0; }
