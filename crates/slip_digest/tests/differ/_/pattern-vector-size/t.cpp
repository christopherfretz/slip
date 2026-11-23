#include "side.h"
// WP-124 item 4 (Kiln WP-3027 "Reachable"): changed in a template nothing instantiates; the AST holds it.
template<class T> struct S {
#ifdef B
  typedef int v __attribute__((vector_size(sizeof(T) * 8)));
#else
  typedef int v __attribute__((vector_size(sizeof(T) * 4)));
#endif
};
