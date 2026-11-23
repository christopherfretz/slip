#include "side.h"
// WP-124 item 4: changed in a template nothing instantiates; the AST holds it.
template<class T> struct S {
#ifdef B
  virtual void f();
#else
  void f();
#endif
};
