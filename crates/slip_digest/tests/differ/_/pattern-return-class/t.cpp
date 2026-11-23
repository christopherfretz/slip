#include "side.h"
// WP-124 item 4 (Kiln WP-3027 A7 declared_returns): changed in a template nothing instantiates; the AST holds it.
template<class T> struct S {
#ifdef B
  decltype(auto) f() { return T(); }
#else
  auto f() { return T(); }
#endif
};
