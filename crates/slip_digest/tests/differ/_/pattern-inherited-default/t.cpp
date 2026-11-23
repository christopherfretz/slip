#include "side.h"
// WP-124 item 4 (Kiln WP-3027 "Reachable"): changed in a template nothing instantiates; the AST holds it. The default is on the member template's first declaration.
template<class T> struct S {
#ifdef B
  template<class U = long> void f();
#else
  template<class U = int> void f();
#endif
};
template<class T> template<class U> void S<T>::f() {}
