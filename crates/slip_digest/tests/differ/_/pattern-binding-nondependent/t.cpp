#include "side.h"
// WP-124 item 4 (Kiln WP-3027 A7): changed in a template nothing instantiates; the AST holds it.
struct Pr { int a, b; };
template<class T> void f() { Pr p;
#ifdef B
  auto& [a, b] = p;
#else
  auto [a, b] = p;
#endif
}
