#include "side.h"
// WP-124 item 4: changed in a template nothing instantiates; the AST holds it.
template<class T> struct S {
#ifdef B
  static inline constinit int v = 1;
#else
  static inline int v = 1;
#endif
};
