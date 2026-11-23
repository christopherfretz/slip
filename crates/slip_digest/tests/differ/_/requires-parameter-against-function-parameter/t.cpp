#include "side.h"
// §4.4 `decl parm` in a walk in full: a requires-expression's parameter and
// the function's parameter at the same position are two entities. The body
// is walked in full (not as a profile, whose `decl parm` has the
// in-requires bit), so the two references must still differ.
template<class T> bool f(T a) {
#ifdef B
  return requires (T b) { a; };
#else
  return requires (T b) { b; };
#endif
}
