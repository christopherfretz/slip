#include "side.h"
// std: c++26
// WP-124 item 4 (Kiln WP-3027 "Reachable"): changed in a template nothing instantiates; the AST holds it.
template<class T> struct S {
#ifdef B
  [[=2]] int m;
#else
  [[=1]] int m;
#endif
};
