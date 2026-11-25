#include "side.h"
// A limit by name (spec/digest.md §2.3, WP-156; WP-153 §13.2 B7 item 1):
// a template candidate that lost to a non-template is not instantiated
// by the front end, so its signature's operand is in no unit (`n::S::s`
// on side A, `Base::s` on side B under the rename): equal.
struct Base { static int s(); };
namespace n { struct S : Base {
#ifdef B
  static int h();
#else
  static int s();
#endif
}; }
template <class T> auto lost(T) -> decltype(T::s());
int lost(n::S);
int u = sizeof(lost(n::S()));
