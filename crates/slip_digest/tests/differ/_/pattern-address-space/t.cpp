#include "side.h"
// WP-124 item 4 (Kiln WP-3027 "Reachable"): changed in a template nothing instantiates; the AST holds it.
template<int N> void f() {
#ifdef B
  int __attribute__((address_space(N + 1))) *p = nullptr;
#else
  int __attribute__((address_space(N))) *p = nullptr;
#endif
}
