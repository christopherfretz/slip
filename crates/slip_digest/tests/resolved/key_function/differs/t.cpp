#include "side.h"
// digest-kinds §11.1 property 4: the key function is the first non-pure
// virtual function not inline at the class's end: `f` on side A, `g` on
// side B. The resolved unit differs; the class's own unit does not.
struct S {
#ifdef B
  virtual void f() {}
#else
  virtual void f();
#endif
  virtual void g();
};
