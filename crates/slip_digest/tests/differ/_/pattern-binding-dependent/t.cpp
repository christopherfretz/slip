#include "side.h"
// WP-124 item 4 (Kiln WP-3027 A7): changed in a template nothing instantiates; the AST holds it.
template<class T> void f(T t) {
#ifdef B
  auto& [a, b] = t;
#else
  auto [a, b] = t;
#endif
}
