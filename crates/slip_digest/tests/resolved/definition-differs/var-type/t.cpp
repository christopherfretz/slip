#include "side.h"
// digest-kinds §11.9 row `ty-type` of a variable whose pattern's type is
// a placeholder. The initialiser is dependent, so the pattern keeps
// `auto` and the instantiation deduces its type: only the pattern and
// `definition` change. (With `= 1L` Kiln deduces the pattern's type
// itself, and the own unit writes it.)
template <class T> struct A {
#ifdef B
  static inline auto s = 1L + T();
#else
  static inline auto s = 1 + T();
#endif
};
long h() { return A<int>::s; }
