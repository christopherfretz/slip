#include "side.h"
// WP-124 item 4: changed in a template nothing instantiates; the AST holds it.
template<class T> int f() { int k = 1;
#ifdef B
  auto l = [&] { return k; };
#else
  auto l = [=] { return k; };
#endif
  return l(); }
